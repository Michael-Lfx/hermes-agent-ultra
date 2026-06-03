//! Local paths under `$HERMES_HOME/insights/`.

use std::path::{Path, PathBuf};

pub fn state_dir(hermes_home: &Path) -> PathBuf {
    hermes_home.join("insights")
}

pub fn outbox_path(hermes_home: &Path) -> PathBuf {
    state_dir(hermes_home).join("outbox.db")
}

pub fn installation_id_path(hermes_home: &Path) -> PathBuf {
    state_dir(hermes_home).join("installation_id")
}

pub fn audit_path(hermes_home: &Path) -> PathBuf {
    state_dir(hermes_home).join("audit.jsonl")
}

/// Last batch POST body written by `flush` (for upload debugging).
pub fn last_batch_path(hermes_home: &Path) -> PathBuf {
    state_dir(hermes_home).join("last_batch.json")
}

pub fn skill_state_path(hermes_home: &Path) -> PathBuf {
    state_dir(hermes_home).join("skill_state.json")
}

pub fn ensure_state_dir(hermes_home: &Path) -> std::io::Result<PathBuf> {
    let dir = state_dir(hermes_home);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Load or create a persistent pseudo-anonymous installation UUID.
pub fn load_or_create_installation_id(hermes_home: &Path) -> Result<String, String> {
    ensure_state_dir(hermes_home).map_err(|e| e.to_string())?;
    let path = installation_id_path(hermes_home);
    if let Ok(raw) = std::fs::read_to_string(&path) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::write(&path, format!("{id}\n")).map_err(|e| e.to_string())?;
    Ok(id)
}
