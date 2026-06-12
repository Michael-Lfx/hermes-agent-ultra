//! WhatsApp DM/group access policy and mention gating.

use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;

use super::config::WhatsAppConfig;

#[derive(Debug, Clone)]
pub struct WhatsAppPolicy {
    whatsapp_mode: String,
    dm_policy: String,
    allow_from: HashSet<String>,
    group_policy: String,
    group_allow_from: HashSet<String>,
    require_mention: bool,
    mention_patterns: Vec<Regex>,
    free_response_chats: HashSet<String>,
}

impl WhatsAppPolicy {
    pub fn from_config(cfg: &WhatsAppConfig) -> Self {
        Self {
            whatsapp_mode: cfg.whatsapp_mode(),
            dm_policy: cfg.dm_policy.clone(),
            allow_from: cfg.allow_from.iter().cloned().collect(),
            group_policy: cfg.group_policy.clone(),
            group_allow_from: cfg.group_allow_from.iter().cloned().collect(),
            require_mention: cfg.require_mention.unwrap_or_else(|| {
                std::env::var("WHATSAPP_REQUIRE_MENTION")
                    .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes" | "on"))
                    .unwrap_or(false)
            }),
            mention_patterns: compile_mention_patterns(&cfg.mention_patterns),
            free_response_chats: cfg.free_response_chats.iter().cloned().collect(),
        }
    }

    pub fn enforces_own_access_policy(&self) -> bool {
        true
    }

    pub fn is_broadcast_chat(chat_id: &str) -> bool {
        if chat_id.is_empty() {
            return false;
        }
        let cid = chat_id.trim().to_lowercase();
        cid == "status@broadcast" || cid.ends_with("@broadcast") || cid.ends_with("@newsletter")
    }

    pub fn should_process_message(&self, data: &Value) -> bool {
        let chat_id_raw = data.get("chatId").and_then(|v| v.as_str()).unwrap_or("");
        if Self::is_broadcast_chat(chat_id_raw) {
            return false;
        }

        let is_group = data
            .get("isGroup")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_group {
            if !self.is_group_allowed(chat_id_raw) {
                return false;
            }
        } else if self.whatsapp_mode == "self-chat" {
            // rust_client already filters non-owner chats; do not re-drop owner DMs here.
            return true;
        } else {
            let sender_id = data
                .get("senderId")
                .or_else(|| data.get("from"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return self.is_dm_allowed(sender_id);
        }

        let chat_id = chat_id_raw.to_string();
        if self.free_response_chats.contains(&chat_id) {
            return true;
        }
        if !self.require_mention {
            return true;
        }
        let body = data
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if body.starts_with('/') {
            return true;
        }
        if message_is_reply_to_bot(data) {
            return true;
        }
        if message_mentions_bot(data) {
            return true;
        }
        self.message_matches_mention_patterns(body)
    }

    pub fn clean_bot_mention_text(&self, text: &str, data: &Value) -> String {
        let mut cleaned = text.to_string();
        for bot_id in bot_ids_from_message(data) {
            let bare = bot_id.split('@').next().unwrap_or("");
            if bare.is_empty() {
                continue;
            }
            let pattern = format!(r"@{}[,:\-]*\s*", regex::escape(bare));
            if let Ok(re) = Regex::new(&pattern) {
                cleaned = re.replace_all(&cleaned, "").into_owned();
            }
        }
        let trimmed = cleaned.trim();
        if trimmed.is_empty() {
            text.to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn is_dm_allowed(&self, sender_id: &str) -> bool {
        match self.dm_policy.as_str() {
            "disabled" => false,
            "allowlist" => self.matches_user_allowlist(&self.allow_from, sender_id),
            _ => true,
        }
    }

    fn is_group_allowed(&self, chat_id: &str) -> bool {
        match self.group_policy.as_str() {
            "disabled" => false,
            "allowlist" => self.matches_user_allowlist(&self.group_allow_from, chat_id),
            _ => true,
        }
    }

    fn matches_user_allowlist(&self, allow: &HashSet<String>, id: &str) -> bool {
        if allow.contains("*") {
            return true;
        }
        if allow.contains(id) {
            return true;
        }
        let normalized = crate::whatsapp_identity::normalize_whatsapp_identifier(id);
        if allow.contains(&normalized) {
            return true;
        }
        allow.iter().any(|entry| {
            crate::whatsapp_identity::normalize_whatsapp_identifier(entry) == normalized
        })
    }

    fn message_matches_mention_patterns(&self, body: &str) -> bool {
        self.mention_patterns.iter().any(|re| re.is_match(body))
    }
}

fn compile_mention_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| Regex::new(&format!("(?i){p}")).ok())
        .collect()
}

fn normalize_whatsapp_id(value: &str) -> String {
    if value.contains(':') && value.contains('@') {
        value.replace(':', "@")
    } else {
        value.to_string()
    }
}

fn bot_ids_from_message(data: &Value) -> HashSet<String> {
    data.get("botIds")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str())
                .map(normalize_whatsapp_id)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn message_is_reply_to_bot(data: &Value) -> bool {
    let quoted = data
        .get("quotedParticipant")
        .and_then(|v| v.as_str())
        .map(normalize_whatsapp_id)
        .unwrap_or_default();
    if quoted.is_empty() {
        return false;
    }
    bot_ids_from_message(data).contains(&quoted)
}

fn message_mentions_bot(data: &Value) -> bool {
    let bot_ids = bot_ids_from_message(data);
    if bot_ids.is_empty() {
        return false;
    }
    let mentioned: HashSet<String> = data
        .get("mentionedIds")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str())
                .map(normalize_whatsapp_id)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !mentioned.is_disjoint(&bot_ids) {
        return true;
    }
    let body = data
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    bot_ids.iter().any(|bot_id| {
        let bare = bot_id.split('@').next().unwrap_or("").to_lowercase();
        !bare.is_empty() && (body.contains(&format!("@{bare}")) || body.contains(&bare))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy(extra: WhatsAppConfig) -> WhatsAppPolicy {
        WhatsAppPolicy::from_config(&extra)
    }

    #[test]
    fn dm_disabled() {
        let mut cfg = WhatsAppConfig::default();
        cfg.dm_policy = "disabled".into();
        let p = policy(cfg);
        assert!(!p.should_process_message(&json!({"isGroup": false, "senderId": "1"})));
    }

    #[test]
    fn dm_allowlist() {
        let mut cfg = WhatsAppConfig::default();
        cfg.dm_policy = "allowlist".into();
        cfg.allow_from = vec!["15551234567".into()];
        let p = policy(cfg);
        assert!(p.should_process_message(&json!({"isGroup": false, "senderId": "15551234567"})));
        assert!(!p.should_process_message(&json!({"isGroup": false, "senderId": "999"})));
    }

    #[test]
    fn require_mention_blocks_plain_group() {
        let mut cfg = WhatsAppConfig::default();
        cfg.require_mention = Some(true);
        let p = policy(cfg);
        assert!(!p.should_process_message(&json!({
            "isGroup": true,
            "chatId": "120@g.us",
            "body": "hello"
        })));
    }

    #[test]
    fn slash_bypass() {
        let mut cfg = WhatsAppConfig::default();
        cfg.require_mention = Some(true);
        let p = policy(cfg);
        assert!(p.should_process_message(&json!({
            "isGroup": true,
            "chatId": "120@g.us",
            "body": "/status"
        })));
    }

    #[test]
    fn drops_status_broadcast() {
        let p = policy(WhatsAppConfig::default());
        assert!(!p.should_process_message(&json!({
            "chatId": "status@broadcast",
            "body": "hi"
        })));
    }
}
