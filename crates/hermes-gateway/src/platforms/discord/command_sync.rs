//! Safe slash-command sync (P2-11): diff global commands vs desired set.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use hermes_config::paths::hermes_home;
use hermes_core::errors::GatewayError;

use super::gateway_loop::DiscordInner;
use super::types::SlashCommand;

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

const DISCORD_COMMANDS_STATE_DIR: &str = "discord";
const DISCORD_COMMANDS_STATE_FILE: &str = "slash_commands_state.json";

/// Discord global application command as returned by GET /applications/{id}/commands.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalCommandRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "type", default = "default_cmd_type")]
    pub command_type: u8,
    #[serde(default)]
    pub options: Vec<serde_json::Value>,
}

fn default_cmd_type() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CommandSyncSummary {
    pub total: usize,
    pub unchanged: usize,
    pub updated: usize,
    pub recreated: usize,
    pub created: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SlashCommandsStateFile {
    #[serde(default)]
    commands: HashMap<String, StoredCommandState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCommandState {
    id: String,
    hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandFingerprint {
    name: String,
    description: String,
    command_type: u8,
    options_json: String,
}

impl CommandFingerprint {
    fn from_slash(cmd: &SlashCommand) -> Self {
        let options_json = match &cmd.options {
            None => "[]".to_string(),
            Some(values) if values.is_empty() => "[]".to_string(),
            Some(values) => serde_json::to_string(values).unwrap_or_else(|_| "[]".into()),
        };
        Self {
            name: cmd.name.clone(),
            description: cmd.description.clone(),
            command_type: cmd.command_type,
            options_json,
        }
    }

    fn from_global(cmd: &GlobalCommandRecord) -> Self {
        let options_json = if cmd.options.is_empty() {
            "[]".to_string()
        } else {
            serde_json::to_string(&cmd.options).unwrap_or_else(|_| "[]".into())
        };
        Self {
            name: cmd.name.clone(),
            description: cmd.description.clone(),
            command_type: cmd.command_type,
            options_json,
        }
    }

    fn hash(&self) -> String {
        let payload = serde_json::json!({
            "name": self.name,
            "description": self.description,
            "type": self.command_type,
            "options": self.options_json,
        });
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        hex_encode(&Sha256::digest(bytes))
    }
}

pub fn slash_commands_state_path() -> PathBuf {
    hermes_home()
        .join(DISCORD_COMMANDS_STATE_DIR)
        .join(DISCORD_COMMANDS_STATE_FILE)
}

fn load_state() -> SlashCommandsStateFile {
    let path = slash_commands_state_path();
    if !path.exists() {
        return SlashCommandsStateFile::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => SlashCommandsStateFile::default(),
    }
}

fn save_state(state: &SlashCommandsStateFile) -> Result<(), GatewayError> {
    let path = slash_commands_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GatewayError::Platform(format!("create slash commands state dir: {e}")))?;
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|e| GatewayError::Platform(format!("serialize slash commands state: {e}")))?;
    std::fs::write(&path, text)
        .map_err(|e| GatewayError::Platform(format!("write slash commands state: {e}")))
}

fn patchable_diff(desired: &CommandFingerprint, existing: &CommandFingerprint) -> bool {
    desired.name == existing.name
        && desired.command_type == existing.command_type
        && (desired.description != existing.description
            || desired.options_json != existing.options_json)
}

impl DiscordInner {
    pub async fn list_global_slash_commands(
        &self,
    ) -> Result<Vec<GlobalCommandRecord>, GatewayError> {
        let app_id = self.config.application_id.as_deref().ok_or_else(|| {
            GatewayError::Platform("application_id required for slash commands".into())
        })?;
        let url = format!(
            "{}/applications/{app_id}/commands",
            self.config.rest_api_base
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("Discord list commands failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!(
                "Discord list commands API error: {text}"
            )));
        }
        resp.json().await.map_err(|e| {
            GatewayError::SendFailed(format!("Discord list commands parse error: {e}"))
        })
    }

    async fn create_global_slash_command(
        &self,
        command: &SlashCommand,
    ) -> Result<GlobalCommandRecord, GatewayError> {
        let app_id = self.config.application_id.as_deref().ok_or_else(|| {
            GatewayError::Platform("application_id required for slash commands".into())
        })?;
        let url = format!(
            "{}/applications/{app_id}/commands",
            self.config.rest_api_base
        );
        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(command)
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("Discord create command failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!(
                "Discord create command API error: {text}"
            )));
        }
        resp.json().await.map_err(|e| {
            GatewayError::SendFailed(format!("Discord create command parse error: {e}"))
        })
    }

    async fn patch_global_slash_command(
        &self,
        command_id: &str,
        command: &SlashCommand,
    ) -> Result<GlobalCommandRecord, GatewayError> {
        let app_id = self.config.application_id.as_deref().ok_or_else(|| {
            GatewayError::Platform("application_id required for slash commands".into())
        })?;
        let url = format!(
            "{}/applications/{app_id}/commands/{command_id}",
            self.config.rest_api_base
        );
        let resp = self
            .client
            .patch(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(command)
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("Discord patch command failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!(
                "Discord patch command API error: {text}"
            )));
        }
        resp.json().await.map_err(|e| {
            GatewayError::SendFailed(format!("Discord patch command parse error: {e}"))
        })
    }

    async fn delete_global_slash_command(&self, command_id: &str) -> Result<(), GatewayError> {
        let app_id = self.config.application_id.as_deref().ok_or_else(|| {
            GatewayError::Platform("application_id required for slash commands".into())
        })?;
        let url = format!(
            "{}/applications/{app_id}/commands/{command_id}",
            self.config.rest_api_base
        );
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| GatewayError::SendFailed(format!("Discord delete command failed: {e}")))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::SendFailed(format!(
                "Discord delete command API error: {text}"
            )));
        }
        Ok(())
    }

    /// Diff-based global command sync (``DISCORD_COMMAND_SYNC_POLICY=safe``).
    pub async fn safe_sync_slash_commands(
        &self,
        desired: &[SlashCommand],
    ) -> Result<CommandSyncSummary, GatewayError> {
        let existing = self.list_global_slash_commands().await?;
        let mut summary = CommandSyncSummary {
            total: desired.len(),
            ..Default::default()
        };

        let desired_by_name: HashMap<String, SlashCommand> = desired
            .iter()
            .map(|c| (c.name.clone(), c.clone()))
            .collect();
        let existing_by_name: HashMap<String, GlobalCommandRecord> = existing
            .iter()
            .map(|c| (c.name.clone(), c.clone()))
            .collect();

        let desired_names: HashSet<String> = desired_by_name.keys().cloned().collect();

        for (name, record) in &existing_by_name {
            if !desired_names.contains(name) {
                self.delete_global_slash_command(&record.id).await?;
                summary.deleted += 1;
                debug!(command = %name, id = %record.id, "deleted stale slash command");
            }
        }

        let mut state = load_state();

        for (name, cmd) in &desired_by_name {
            let desired_fp = CommandFingerprint::from_slash(cmd);
            let desired_hash = desired_fp.hash();

            if let Some(record) = existing_by_name.get(name) {
                let existing_fp = CommandFingerprint::from_global(record);
                if desired_fp == existing_fp {
                    summary.unchanged += 1;
                    state.commands.insert(
                        name.clone(),
                        StoredCommandState {
                            id: record.id.clone(),
                            hash: desired_hash,
                        },
                    );
                    continue;
                }

                if patchable_diff(&desired_fp, &existing_fp) {
                    match self.patch_global_slash_command(&record.id, cmd).await {
                        Ok(updated) => {
                            summary.updated += 1;
                            state.commands.insert(
                                name.clone(),
                                StoredCommandState {
                                    id: updated.id,
                                    hash: desired_hash,
                                },
                            );
                            continue;
                        }
                        Err(err) => {
                            warn!(
                                command = %name,
                                error = %err,
                                "patch slash command failed; recreating"
                            );
                        }
                    }
                }

                if let Err(err) = self.delete_global_slash_command(&record.id).await {
                    warn!(command = %name, error = %err, "delete before recreate failed");
                }
                let created = self.create_global_slash_command(cmd).await?;
                summary.recreated += 1;
                state.commands.insert(
                    name.clone(),
                    StoredCommandState {
                        id: created.id,
                        hash: desired_hash,
                    },
                );
            } else {
                let created = self.create_global_slash_command(cmd).await?;
                summary.created += 1;
                state.commands.insert(
                    name.clone(),
                    StoredCommandState {
                        id: created.id,
                        hash: desired_hash,
                    },
                );
            }
        }

        save_state(&state)?;
        info!(
            total = summary.total,
            unchanged = summary.unchanged,
            updated = summary.updated,
            recreated = summary.recreated,
            created = summary.created,
            deleted = summary.deleted,
            "Discord safe slash command sync complete"
        );
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_diff_detects_description_change() {
        let a = CommandFingerprint {
            name: "help".into(),
            description: "old".into(),
            command_type: 1,
            options_json: "[]".into(),
        };
        let b = CommandFingerprint {
            description: "new".into(),
            ..a.clone()
        };
        assert!(patchable_diff(&b, &a));
        assert_ne!(a, b);
    }
}
