//! WhatsApp session paths and pairing state (wa-rs SQLite backend).

use std::path::{Path, PathBuf};

const PAIRED_MARKER: &str = ".paired";
const LEGACY_CREDS: &str = "creds.json";

pub fn session_db_path(session_path: &Path) -> PathBuf {
    session_path.join("whatsapp.db")
}

pub fn paired_marker_path(session_path: &Path) -> PathBuf {
    session_path.join(PAIRED_MARKER)
}

/// Legacy Baileys credential file (Node bridge). Kept for migration hints only.
pub fn legacy_creds_path(session_path: &Path) -> PathBuf {
    session_path.join(LEGACY_CREDS)
}

pub fn is_paired(session_path: &Path) -> bool {
    paired_marker_path(session_path).exists()
}

pub fn mark_paired(session_path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(session_path)?;
    std::fs::write(paired_marker_path(session_path), "1")
}

pub fn has_legacy_baileys_session(session_path: &Path) -> bool {
    legacy_creds_path(session_path).exists()
}

pub fn ensure_session_dir(session_path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(session_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn paired_marker_roundtrip() {
        let dir = TempDir::new().unwrap();
        let session = dir.path().join("session");
        assert!(!is_paired(&session));
        mark_paired(&session).unwrap();
        assert!(is_paired(&session));
        assert!(session_db_path(&session).ends_with("whatsapp.db"));
    }
}
