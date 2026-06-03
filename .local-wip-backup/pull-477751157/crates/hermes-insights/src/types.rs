//! REST API payload types (aligned with docs/insights/SERVER_IMPLEMENTATION.md).

use serde::{Deserialize, Serialize};

/// Consent document version shown on `hermes contribute enable`.
pub const INSIGHTS_CONSENT_VERSION: &str = "2026-05-29";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionType {
    InterestSnapshot,
    SkillPattern,
}

impl ContributionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InterestSnapshot => "interest_snapshot",
            Self::SkillPattern => "skill_pattern",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionEnvelope {
    #[serde(rename = "type")]
    pub kind: String,
    pub collected_at: String,
    pub content_hash: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionBatch {
    pub batch_id: String,
    pub consent_version: String,
    pub contributions: Vec<ContributionEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestTopicFingerprint {
    /// Human-readable cohort key (`lang:rust`, `topic:beijing-dialect`). Never `interest:<hex>`.
    pub topic_key: String,
    /// Sanitized display label for ops UI (primary).
    pub label_redacted: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_redacted: Option<String>,
    pub namespace: String,
    pub weight_band: String,
    pub evidence_band: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taxonomy_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestFingerprint {
    pub topics: Vec<InterestTopicFingerprint>,
    /// Co-occurring sanitized labels (not local ids).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub co_topics: Vec<String>,
    pub collected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStructure {
    pub headings: Vec<String>,
    pub step_count: u32,
    pub mentions_subagent: bool,
    pub mentions_cron: bool,
    pub mentions_mcp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTriggerHints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slash_command: Option<String>,
    #[serde(default)]
    pub from_background_review: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillProvenance {
    AgentCreated,
    UserCreated,
}

fn default_payload_schema_version() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPattern {
    /// Bumps `content_hash` when upload schema changes (v3 adds templates/scripts/assets files).
    #[serde(default = "default_payload_schema_version")]
    pub payload_schema_version: u32,
    /// Dedup fingerprint for server; ops UI should use `display_name` + text fields.
    pub pattern_id: String,
    /// Sanitized skill title from frontmatter (ops primary).
    pub display_name: String,
    pub name_slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description_redacted: String,
    pub structure: SkillStructure,
    pub tool_chain: Vec<String>,
    pub trigger_hints: SkillTriggerHints,
    pub provenance: SkillProvenance,
    pub content_version: String,
    /// Sanitized POI labels linked at upload time (not local topic ids).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_interest_labels: Vec<String>,
    /// Sanitized SKILL.md body (main sections; auxiliary files are in `references_redacted`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redacted_body: Option<String>,
    /// Sanitized text files from skill `references/`, `templates/`, `scripts/`, `assets/`.
    #[serde(default)]
    pub references_redacted: Vec<SkillReferenceSnippet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillReferenceSnippet {
    /// e.g. `references/api-guide.md`, `scripts/search.py`, `templates/report.md`
    pub relative_path: String,
    pub content_redacted: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchUploadResponse {
    #[serde(default, alias = "accepted_count", alias = "acceptedCount")]
    pub accepted: u32,
    #[serde(default, alias = "duplicate_count", alias = "duplicateCount")]
    pub duplicates: u32,
    #[serde(default)]
    pub rejected: Vec<RejectedContribution>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RejectedContribution {
    pub content_hash: String,
    pub reason: String,
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

pub fn envelope_from_value(
    kind: ContributionType,
    collected_at: &str,
    payload: &impl Serialize,
) -> Result<ContributionEnvelope, String> {
    let payload_value =
        serde_json::to_value(payload).map_err(|e| format!("serialize payload: {e}"))?;
    let canonical =
        serde_json::to_string(&payload_value).map_err(|e| format!("canonical payload: {e}"))?;
    Ok(ContributionEnvelope {
        kind: kind.as_str().to_string(),
        collected_at: collected_at.to_string(),
        content_hash: sha256_hex(canonical.as_bytes()),
        payload: payload_value,
    })
}

/// Drop duplicate `skill_pattern` rows in one batch (same `pattern_id`, keep last).
pub fn dedupe_batch_contributions(contribs: Vec<ContributionEnvelope>) -> Vec<ContributionEnvelope> {
    let mut skill_idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(contribs.len());
    for env in contribs {
        if env.kind != ContributionType::SkillPattern.as_str() {
            out.push(env);
            continue;
        }
        let Some(pid) = skill_pattern_id(&env.payload) else {
            out.push(env);
            continue;
        };
        if let Some(&idx) = skill_idx.get(&pid) {
            out[idx] = env;
        } else {
            skill_idx.insert(pid, out.len());
            out.push(env);
        }
    }
    out
}

pub fn skill_pattern_id(payload: &serde_json::Value) -> Option<String> {
    payload.get("pattern_id")?.as_str().map(str::to_string)
}
