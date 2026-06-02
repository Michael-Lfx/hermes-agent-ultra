//! Browser login-state persistence for agent-browser / CDP backends.
//!
//! Mirrors Python `browser_camofox_state.py`: profile-scoped directories under
//! `$HERMES_HOME/browser_auth/agent-browser/` with stable session identities.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const AUTH_DIR: &str = "browser_auth";
const AUTH_SUBDIR: &str = "agent-browser";

/// Root directory for Hermes-managed agent-browser auth profiles.
pub fn auth_root() -> PathBuf {
    hermes_config::paths::hermes_home()
        .join(AUTH_DIR)
        .join(AUTH_SUBDIR)
}

/// Whether login cookies/profile data should persist across agent tasks.
pub fn persist_login_enabled() -> bool {
    match std::env::var("HERMES_BROWSER_PERSIST_LOGIN")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        Some(_) => true,
        None => true,
    }
}

/// Active auth profile key (Hermes profile / site bucket). Override with `HERMES_BROWSER_AUTH_PROFILE`.
pub fn auth_profile_key() -> String {
    std::env::var("HERMES_BROWSER_AUTH_PROFILE")
        .ok()
        .map(|v| sanitize_key(&v))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

pub fn sanitize_key(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "default".to_string();
    }
    trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Persistent Chromium profile directory for the given auth profile key.
pub fn profile_directory(profile_key: &str) -> PathBuf {
    auth_root().join(sanitize_key(profile_key))
}

/// Stable agent-browser `--session` name for a logical scope (usually task_id).
pub fn stable_session_name(profile_key: &str, logical_scope: &str) -> String {
    let scope = sanitize_key(logical_scope);
    let root = auth_root().display().to_string();
    let raw = format!(
        "agent-browser-session:{root}:{}:{scope}",
        sanitize_key(profile_key)
    );
    let digest = Sha256::digest(raw.as_bytes());
    format!("hermes_{}", hex_prefix(&digest, 16))
}

/// Ephemeral session name (legacy behaviour when persistence is disabled).
pub fn ephemeral_session_name() -> String {
    format!("h_{}", &uuid::Uuid::new_v4().simple().to_string()[..10])
}

pub struct BrowserAuthContext {
    pub enabled: bool,
    pub profile_key: String,
    pub profile_dir: PathBuf,
    pub session_name: String,
}

impl BrowserAuthContext {
    pub fn for_scope(logical_scope: &str) -> Self {
        let profile_key = auth_profile_key();
        if persist_login_enabled() {
            let profile_dir = profile_directory(&profile_key);
            let _ = std::fs::create_dir_all(&profile_dir);
            let session_name = stable_session_name(&profile_key, logical_scope);
            Self {
                enabled: true,
                profile_key,
                profile_dir,
                session_name,
            }
        } else {
            let profile_dir = profile_directory(&profile_key);
            Self {
                enabled: false,
                profile_key,
                profile_dir,
                session_name: ephemeral_session_name(),
            }
        }
    }

    pub fn apply_to_command(&self, cmd: &mut tokio::process::Command) {
        if !self.enabled {
            return;
        }
        cmd.arg("--profile")
            .arg(self.profile_dir.display().to_string());
        cmd.env("AGENT_BROWSER_PROFILE", &self.profile_dir);
        cmd.env("AGENT_BROWSER_SESSION_NAME", &self.session_name);
        if std::env::var("AGENT_BROWSER_HEADED").is_err() && cfg!(windows) {
            cmd.env("AGENT_BROWSER_HEADED", "1");
        }
    }

    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "persist_login": self.enabled,
            "auth_profile": self.profile_key,
            "profile_dir": self.profile_dir.display().to_string(),
            "session_name": self.session_name,
        })
    }
}

/// CDP Chrome user-data-dir when auto-starting with persistence enabled.
pub fn cdp_user_data_dir(port: u16) -> PathBuf {
    if persist_login_enabled() {
        profile_directory(&auth_profile_key())
    } else {
        std::env::temp_dir().join(format!("hermes-chrome-debug-{port}"))
    }
}

pub fn ensure_profile_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

fn hex_prefix(bytes: &[u8], n: usize) -> String {
    bytes
        .iter()
        .take(n)
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_session_is_deterministic() {
        let a = stable_session_name("default", "task-1");
        let b = stable_session_name("default", "task-1");
        assert_eq!(a, b);
        assert!(a.starts_with("hermes_"));
    }

    #[test]
    fn different_scopes_get_different_sessions() {
        let a = stable_session_name("default", "task-1");
        let b = stable_session_name("default", "task-2");
        assert_ne!(a, b);
    }

    #[test]
    fn sanitize_profile_key_strips_unsafe_chars() {
        assert_eq!(sanitize_key("site/a:b"), "site_a_b");
        assert_eq!(sanitize_key(""), "default");
    }

    #[test]
    fn persist_login_env_toggle() {
        let prev = std::env::var("HERMES_BROWSER_PERSIST_LOGIN").ok();
        unsafe {
            std::env::set_var("HERMES_BROWSER_PERSIST_LOGIN", "0");
        }
        assert!(!persist_login_enabled());
        unsafe {
            std::env::set_var("HERMES_BROWSER_PERSIST_LOGIN", "1");
        }
        assert!(persist_login_enabled());
        unsafe {
            match prev {
                Some(v) => std::env::set_var("HERMES_BROWSER_PERSIST_LOGIN", v),
                None => std::env::remove_var("HERMES_BROWSER_PERSIST_LOGIN"),
            }
        }
    }
}
