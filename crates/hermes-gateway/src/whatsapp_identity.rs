//! Canonical WhatsApp sender identity across phone-JID and LID variants.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use std::sync::LazyLock;
use tracing::debug;

static SAFE_IDENTIFIER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9@.+\-]+$").expect("valid regex"));

/// Strip WhatsApp JID/LID syntax down to a bare numeric identifier.
pub fn normalize_whatsapp_identifier(value: &str) -> String {
    let mut s = value.trim().to_string();
    if s.starts_with('+') {
        s = s[1..].to_string();
    }
    if let Some((head, _)) = s.split_once(':') {
        s = head.to_string();
    }
    if let Some((head, _)) = s.split_once('@') {
        s = head.to_string();
    }
    s
}

fn session_dir(session_root: Option<&Path>) -> PathBuf {
    session_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| hermes_config::hermes_home().join("whatsapp").join("session"))
}

/// Resolve all phone/LID aliases reachable from mapping files.
pub fn expand_whatsapp_aliases(identifier: &str, session_root: Option<&Path>) -> HashSet<String> {
    let normalized = normalize_whatsapp_identifier(identifier);
    if normalized.is_empty() {
        return HashSet::new();
    }

    let dir = session_dir(session_root);
    let mut resolved = HashSet::new();
    let mut queue = vec![normalized.clone()];

    while let Some(current) = queue.pop() {
        if current.is_empty() || resolved.contains(&current) {
            continue;
        }
        if !SAFE_IDENTIFIER_RE.is_match(&current) {
            continue;
        }
        resolved.insert(current.clone());
        for suffix in ["", "_reverse"] {
            let mapping_path = dir.join(format!("lid-mapping-{current}{suffix}.json"));
            if !mapping_path.exists() {
                continue;
            }
            match std::fs::read_to_string(&mapping_path) {
                Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(value) => {
                        let mapped = normalize_whatsapp_identifier(value.as_str().unwrap_or(""));
                        if !mapped.is_empty() && !resolved.contains(&mapped) {
                            queue.push(mapped);
                        }
                    }
                    Err(e) => debug!("whatsapp_identity: failed to parse {:?}: {e}", mapping_path),
                },
                Err(e) => debug!("whatsapp_identity: failed to read {:?}: {e}", mapping_path),
            }
        }
    }

    resolved
}

/// Pick a stable canonical identity across alias variants.
pub fn canonical_whatsapp_identifier(identifier: &str, session_root: Option<&Path>) -> String {
    let normalized = normalize_whatsapp_identifier(identifier);
    if normalized.is_empty() {
        return String::new();
    }
    let aliases = expand_whatsapp_aliases(&normalized, session_root);
    if aliases.is_empty() {
        return normalized;
    }
    aliases
        .into_iter()
        .min_by_key(|candidate| (candidate.len(), candidate.clone()))
        .unwrap_or(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn normalize_strips_jid() {
        assert_eq!(
            normalize_whatsapp_identifier("60123456789@s.whatsapp.net"),
            "60123456789"
        );
        assert_eq!(normalize_whatsapp_identifier("60123456789@lid"), "60123456789");
        assert_eq!(normalize_whatsapp_identifier("+15551234567"), "15551234567");
    }

    #[test]
    fn canonical_without_mapping_returns_normalized() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            canonical_whatsapp_identifier("15551234567@s.whatsapp.net", Some(dir.path())),
            "15551234567"
        );
    }

    #[test]
    fn expand_transitive_mapping() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("lid-mapping-999999999999999.json"),
            "\"15551234567\"",
        )
        .unwrap();
        let aliases = expand_whatsapp_aliases("999999999999999@lid", Some(dir.path()));
        assert!(aliases.contains("999999999999999"));
        assert!(aliases.contains("15551234567"));
        assert_eq!(
            canonical_whatsapp_identifier("999999999999999@lid", Some(dir.path())),
            "15551234567"
        );
    }
}
