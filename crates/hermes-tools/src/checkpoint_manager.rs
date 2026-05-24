//! Filesystem checkpoint manager — parity with Python `tools/checkpoint_manager.py`.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// Shadow project id: first 16 hex chars of SHA-256(abs path).
pub fn checkpoint_shadow_dir_id(abs_path_str: &str) -> String {
    let digest = Sha256::digest(abs_path_str.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Transparent filesystem snapshots before mutating tools.
#[derive(Debug)]
pub struct CheckpointManager {
    enabled: bool,
    store_root: PathBuf,
    project_id: Option<String>,
    workdir: PathBuf,
}

impl CheckpointManager {
    pub fn new(
        enabled: bool,
        hermes_home: Option<&Path>,
        workdir: impl AsRef<Path>,
    ) -> Self {
        let home = hermes_home
            .map(Path::to_path_buf)
            .or_else(|| Some(hermes_config::paths::hermes_home()));
        let store_root = home
            .map(|h| h.join("checkpoints").join("store"))
            .unwrap_or_else(|| PathBuf::from(".hermes/checkpoints/store"));
        let workdir = workdir.as_ref().canonicalize().unwrap_or_else(|_| {
            workdir.as_ref().to_path_buf()
        });
        let project_id = workdir
            .to_str()
            .map(checkpoint_shadow_dir_id);
        Self {
            enabled,
            store_root,
            project_id,
            workdir,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn new_turn(&mut self) {
        // Per-turn dedup hook — snapshots are taken per mutating tool call.
    }

    /// Ensure a checkpoint exists for `path` before mutation.
    pub fn ensure_checkpoint(&mut self, path: &Path, label: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let _ = (path, label);
        self.snapshot_worktree("pre-mutation")
    }

    pub fn restore_latest(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let Some(project_id) = self.project_id.as_ref() else {
            return Err("checkpoint: no project id".into());
        };
        let git_dir = self.store_root.join("objects").parent().map(|p| {
            p.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.store_root.clone())
        });
        let git_dir = git_dir.unwrap_or_else(|| self.store_root.clone());
        if !git_dir.join("HEAD").exists() {
            return Err("checkpoint: no snapshots yet".into());
        }
        let ref_name = format!("refs/hermes/{project_id}");
        let output = Command::new("git")
            .args([
                "--git-dir",
                git_dir.to_str().ok_or("invalid git dir")?,
                "--work-tree",
                self.workdir.to_str().ok_or("invalid workdir")?,
                "checkout",
                ref_name.as_str(),
                "--",
                ".",
            ])
            .output()
            .map_err(|e| format!("checkpoint restore failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "checkpoint restore: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    fn snapshot_worktree(&self, message: &str) -> Result<(), String> {
        let Some(project_id) = self.project_id.as_ref() else {
            return Ok(());
        };
        std::fs::create_dir_all(&self.store_root).map_err(|e| e.to_string())?;
        let git_dir = &self.store_root;
        if !git_dir.join("HEAD").exists() {
            Command::new("git")
                .args(["init", "--bare"])
                .current_dir(git_dir)
                .output()
                .map_err(|e| format!("git init: {e}"))?;
        }
        let ref_name = format!("refs/hermes/{project_id}");
        let status = Command::new("git")
            .env("GIT_DIR", git_dir)
            .env("GIT_WORK_TREE", &self.workdir)
            .args(["add", "-A"])
            .output()
            .map_err(|e| format!("git add: {e}"))?;
        if !status.status.success() {
            return Err(String::from_utf8_lossy(&status.stderr).into());
        }
        let commit = Command::new("git")
            .env("GIT_DIR", git_dir)
            .env("GIT_WORK_TREE", &self.workdir)
            .args(["commit", "-m", message, "--allow-empty"])
            .output()
            .map_err(|e| format!("git commit: {e}"))?;
        if !commit.status.success() {
            return Err(String::from_utf8_lossy(&commit.stderr).into());
        }
        let update_ref = Command::new("git")
            .env("GIT_DIR", git_dir)
            .args(["update-ref", ref_name.as_str(), "HEAD"])
            .output()
            .map_err(|e| format!("git update-ref: {e}"))?;
        if !update_ref.status.success() {
            return Err(String::from_utf8_lossy(&update_ref.stderr).into());
        }
        Ok(())
    }
}
