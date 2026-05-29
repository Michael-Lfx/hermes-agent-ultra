//! Slash command catalog and autocomplete helpers (P2-12).

use std::collections::HashSet;

use hermes_config::paths::skills_dir;
use hermes_core::types::SkillMeta;
use hermes_skills::{FileSkillStore, SkillStore};
use tracing::warn;

use super::types::{extended_slash_commands, SlashCommand, SlashCommandChoice, SlashCommandOption};

const DISCORD_MAX_APPLICATION_COMMANDS: usize = 100;
const OPTION_TYPE_STRING: u8 = 3;
const AUTOCOMPLETE_MAX_CHOICES: usize = 25;

/// Build the full desired global slash command set for registration.
pub async fn build_desired_slash_commands() -> Vec<SlashCommand> {
    let mut commands = extended_slash_commands();
    if let Some(skill_cmd) = skill_slash_command().await {
        commands.push(skill_cmd);
    }
    cap_commands(&mut commands);
    commands
}

async fn skill_slash_command() -> Option<SlashCommand> {
    let store = FileSkillStore::new(skills_dir());
    let metas = store.list().await.unwrap_or_default();
    if metas.is_empty() {
        return None;
    }
    Some(SlashCommand {
        name: "skill".into(),
        description: "Run an installed skill by name".into(),
        options: Some(vec![SlashCommandOption {
            name: "name".into(),
            description: "Skill name".into(),
            option_type: OPTION_TYPE_STRING,
            required: Some(true),
            choices: None,
            autocomplete: Some(true),
        }]),
        command_type: 1,
    })
}

fn cap_commands(commands: &mut Vec<SlashCommand>) {
    if commands.len() <= DISCORD_MAX_APPLICATION_COMMANDS {
        return;
    }
    warn!(
        count = commands.len(),
        max = DISCORD_MAX_APPLICATION_COMMANDS,
        "Discord slash command count exceeds limit; truncating extras"
    );
    commands.truncate(DISCORD_MAX_APPLICATION_COMMANDS);
}

/// Format native slash interaction as gateway text (``/model gpt-4``).
pub fn format_slash_command_text(command_name: &str, options: &[super::parse::InteractionOption]) -> String {
    let mut parts = vec![format!("/{command_name}")];
    for opt in options {
        let value = format_option_value(&opt.value);
        if !value.is_empty() {
            parts.push(value);
        }
    }
    parts.join(" ")
}

fn format_option_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.trim().to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Skill name choices for ``/skill`` autocomplete (authorized callers only).
pub async fn skill_autocomplete_choices(prefix: &str) -> Vec<SlashCommandChoice> {
    let store = FileSkillStore::new(skills_dir());
    let metas = store.list().await.unwrap_or_default();
    skill_choices_from_metas(&metas, prefix)
}

pub fn skill_choices_from_metas(metas: &[SkillMeta], prefix: &str) -> Vec<SlashCommandChoice> {
    let prefix_lower = prefix.trim().to_ascii_lowercase();
    let mut seen = HashSet::new();
    let mut choices = Vec::new();
    for meta in metas {
        if !prefix_lower.is_empty() && !meta.name.to_ascii_lowercase().contains(&prefix_lower) {
            continue;
        }
        if !seen.insert(meta.name.clone()) {
            continue;
        }
        let description = meta
            .description
            .as_deref()
            .unwrap_or("Installed skill")
            .chars()
            .take(100)
            .collect::<String>();
        choices.push(SlashCommandChoice {
            name: meta.name.chars().take(100).collect(),
            value: serde_json::Value::String(meta.name.clone()),
        });
        if choices.len() >= AUTOCOMPLETE_MAX_CHOICES {
            break;
        }
        let _ = description;
    }
    choices
}

/// Model/provider autocomplete stubs (expand when model_metadata is wired).
pub fn model_autocomplete_choices(prefix: &str) -> Vec<SlashCommandChoice> {
    let candidates = [
        "gpt-4o",
        "gpt-4o-mini",
        "claude-sonnet-4",
        "claude-opus-4",
        "gemini-2.0-flash",
    ];
    let prefix_lower = prefix.trim().to_ascii_lowercase();
    candidates
        .iter()
        .filter(|name| prefix_lower.is_empty() || name.to_ascii_lowercase().contains(&prefix_lower))
        .take(AUTOCOMPLETE_MAX_CHOICES)
        .map(|name| SlashCommandChoice {
            name: (*name).into(),
            value: serde_json::Value::String((*name).into()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::discord::parse::InteractionOption;

    #[test]
    fn format_slash_with_args() {
        let text = format_slash_command_text(
            "model",
            &[InteractionOption {
                name: "args".into(),
                value: serde_json::Value::String("gpt-4o".into()),
            }],
        );
        assert_eq!(text, "/model gpt-4o");
    }

    #[test]
    fn skill_choices_filter_prefix() {
        let metas = vec![
            SkillMeta {
                name: "alpha".into(),
                description: None,
                category: None,
            },
            SkillMeta {
                name: "beta".into(),
                description: None,
                category: None,
            },
        ];
        let choices = skill_choices_from_metas(&metas, "al");
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].name, "alpha");
    }
}
