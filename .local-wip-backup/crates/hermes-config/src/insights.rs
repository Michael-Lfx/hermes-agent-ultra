//! De-identified insights contribution configuration (client → ops server).

use serde::{Deserialize, Serialize};

/// Controls opt-in upload of anonymized interest/skills data for ops analytics.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct InsightsConfig {
    #[serde(default)]
    pub contribution: InsightsContributionConfig,
}

/// Per-feature contribution toggles and REST transport settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsightsContributionConfig {
    /// Master switch — default off (opt-in).
    #[serde(default)]
    pub enabled: bool,

    /// REST batch ingest URL (e.g. `https://ops.example.com/v1/insights/batch`).
    /// Empty means outbox-only; `flush` skips upload.
    #[serde(default)]
    pub endpoint: String,

    /// Upload de-identified POI / interest fingerprints.
    #[serde(default = "default_upload_interests")]
    pub upload_interests: bool,

    /// Upload de-identified skill patterns (not full SKILL.md by default).
    #[serde(default = "default_upload_skills")]
    pub upload_skills: bool,

    /// Enqueue a snapshot at session end.
    #[serde(default = "default_on_session_end")]
    pub on_session_end: bool,

    /// Minimum age before a new local skill is eligible for contribution.
    #[serde(default = "default_skill_min_age_hours")]
    pub skill_min_age_hours: u32,

    /// When true (default), include sanitized SKILL.md body for ops readability (`references/` stripped).
    #[serde(default = "default_redacted_body")]
    pub redacted_body: bool,

    /// `Authorization: Bearer` credential for the ops server (user JWT or `flowy-` API key).
    /// Prefer env `HERMES_INSIGHTS_TOKEN`; may be set in config.yaml (e.g. hardcoded JWT for now).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "installation_token"
    )]
    pub auth_token: Option<String>,
}

fn default_upload_interests() -> bool {
    true
}

fn default_upload_skills() -> bool {
    true
}

fn default_on_session_end() -> bool {
    true
}

fn default_skill_min_age_hours() -> u32 {
    24
}

fn default_redacted_body() -> bool {
    true
}

impl Default for InsightsContributionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: String::new(),
            upload_interests: default_upload_interests(),
            upload_skills: default_upload_skills(),
            on_session_end: default_on_session_end(),
            skill_min_age_hours: default_skill_min_age_hours(),
            redacted_body: default_redacted_body(),
            auth_token: None,
        }
    }
}

impl InsightsContributionConfig {
    pub fn effective_token(&self) -> Option<String> {
        if let Ok(env) = std::env::var("HERMES_INSIGHTS_TOKEN") {
            let trimmed = env.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        self.auth_token
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    pub fn upload_ready(&self) -> bool {
        self.enabled
            && !self.endpoint.trim().is_empty()
            && self.effective_token().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_opt_in_off() {
        let cfg = InsightsContributionConfig::default();
        assert!(!cfg.enabled);
        assert!(!cfg.upload_ready());
        assert!(cfg.upload_interests);
        assert!(cfg.upload_skills);
    }

    #[test]
    fn upload_ready_requires_bearer_token() {
        let mut cfg = InsightsContributionConfig {
            enabled: true,
            endpoint: "https://ops.example.com/v1/insights/batch".to_string(),
            ..Default::default()
        };
        assert!(!cfg.upload_ready());
        cfg.auth_token = Some("eyJhbGciOiJIUzI1NiJ9.test".to_string());
        assert!(cfg.upload_ready());
    }

    #[test]
    fn auth_token_yaml_alias() {
        let yaml = r#"
enabled: true
endpoint: "https://ops.example.com/v1/insights/batch"
auth_token: "flowy-sk-test"
"#;
        let cfg: InsightsContributionConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            cfg.effective_token().as_deref(),
            Some("flowy-sk-test")
        );
    }
}
