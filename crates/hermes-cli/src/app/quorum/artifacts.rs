use std::path::{Path, PathBuf};

use crate::alpha_runtime::QuorumPolicy;
use hermes_core::AgentError;

use super::super::App;
use super::QuorumVoterOutcome;

impl App {
    pub(super) fn persist_quorum_artifact(
        &self,
        policy: &QuorumPolicy,
        voter_outcomes: &[QuorumVoterOutcome],
    ) -> Result<PathBuf, AgentError> {
        let dir = self.state_root.join("quorum");
        std::fs::create_dir_all(&dir).map_err(|e| {
            AgentError::Io(format!(
                "Failed to create quorum artifact dir {}: {}",
                dir.display(),
                e
            ))
        })?;
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
        let file_name = format!("{}-{}.json", self.session.session_id, timestamp);
        let path = dir.join(file_name);
        let payload = serde_json::json!({
            "session_id": self.session.session_id,
            "saved_at": chrono::Utc::now().to_rfc3339(),
            "policy": policy,
            "model_at_start": self.model.current_model,
            "voters": voter_outcomes,
        });
        let raw = serde_json::to_string_pretty(&payload)
            .map_err(|e| AgentError::Config(format!("Failed to serialize quorum artifact: {e}")))?;
        std::fs::write(&path, raw).map_err(|e| {
            AgentError::Io(format!(
                "Failed to write quorum artifact {}: {}",
                path.display(),
                e
            ))
        })?;
        Ok(path)
    }

    pub(super) fn update_quorum_artifact_with_synthesis(
        path: &Path,
        synthesis: &str,
    ) -> Result<(), AgentError> {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            AgentError::Io(format!(
                "Failed to read quorum artifact {}: {}",
                path.display(),
                e
            ))
        })?;
        let mut payload: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            AgentError::Config(format!(
                "Failed to parse quorum artifact {}: {}",
                path.display(),
                e
            ))
        })?;
        payload["synthesis"] = serde_json::Value::String(synthesis.trim().to_string());
        payload["synthesis_saved_at"] = serde_json::Value::String(chrono::Utc::now().to_rfc3339());
        let updated = serde_json::to_string_pretty(&payload).map_err(|e| {
            AgentError::Config(format!(
                "Failed to serialize quorum synthesis artifact {}: {}",
                path.display(),
                e
            ))
        })?;
        std::fs::write(path, updated).map_err(|e| {
            AgentError::Io(format!(
                "Failed to write quorum synthesis artifact {}: {}",
                path.display(),
                e
            ))
        })
    }
}
