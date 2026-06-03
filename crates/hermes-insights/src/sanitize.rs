//! Sanitization pipeline before enqueue or upload.

use std::sync::LazyLock;

use hermes_intelligence::Redactor;
use regex::Regex;

static HOME_PATH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:~[/\\]|/home/[\w.-]+|C:\\Users\\[\w.-]+\\)").unwrap());
static GIT_REMOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"git@[\w.-]+:[\w./-]+\.git").unwrap());
static ABS_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:^|[\s"'`])([~/][\w./\\-]+|[A-Za-z]:\\[\w\\.-]+)"#).unwrap()
});

const POI_BLOCKLIST: &[&str] = &[
    "user", "assistant", "system", "memory", "interest", "config", "session", "prompt",
];

/// Local-only id (`interest:<hex>`) — meaningless to ops without client DB.
pub fn is_opaque_local_topic_id(topic_id: &str) -> bool {
    let id = topic_id.trim().to_ascii_lowercase();
    let Some(suffix) = id.strip_prefix("interest:") else {
        return false;
    };
    suffix.len() >= 12 && suffix.chars().all(|c| c.is_ascii_hexdigit())
}

/// True if string looks like an opaque hash token, not human text.
pub fn looks_like_opaque_token(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 12 {
        return false;
    }
    t.chars()
        .all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_')
        && t.chars().filter(|c| c.is_ascii_hexdigit()).count() >= t.len() * 2 / 3
}

/// Strip paths, secrets, and PII from free text.
pub fn sanitize_text(text: &str) -> String {
    let redactor = Redactor::new();
    let mut out = redactor.redact(text);
    out = HOME_PATH_RE.replace_all(&out, "{{PATH}}").to_string();
    out = GIT_REMOTE_RE.replace_all(&out, "{{GIT_REMOTE}}").to_string();
    out = ABS_PATH_RE.replace_all(&out, " {{PATH}} ").to_string();
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Returns true if text still looks like it contains sensitive patterns after sanitization.
pub fn contains_residual_pii(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains('@') && lower.contains('.') && !lower.contains("[redacted") {
        return true;
    }
    if lower.contains("sk-") && !lower.contains("[redacted") {
        return true;
    }
    false
}

pub fn weight_band(weight: f64) -> &'static str {
    if weight >= 0.66 {
        "high"
    } else if weight >= 0.33 {
        "med"
    } else {
        "low"
    }
}

pub fn evidence_band(count: u32) -> &'static str {
    if count >= 6 {
        "6+"
    } else if count >= 3 {
        "3-5"
    } else {
        "1-2"
    }
}

pub fn parse_namespace(topic_key: &str) -> String {
    topic_key
        .split_once(':')
        .map(|(ns, _)| ns.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Whether a POI topic id is safe to contribute.
pub fn is_contributable_topic_key(topic_key: &str, label: &str) -> bool {
    let key = topic_key.trim().to_ascii_lowercase();
    if key.is_empty() {
        return false;
    }
    if key.starts_with("path:") || key.starts_with("keyword:") {
        return false;
    }
    let label_lower = label.trim().to_ascii_lowercase();
    if label_lower.is_empty() {
        return false;
    }
    if POI_BLOCKLIST.contains(&label_lower.as_str()) {
        return false;
    }
    true
}

/// Whether a topic row should be written to local `interest.db` (aligned with ops filters).
pub fn is_persistable_local_poi(topic_id: &str, label: &str) -> bool {
    let id = topic_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return false;
    }
    // Bag-of-words and shallow paths are session noise, not durable interests.
    if id.starts_with("keyword:") || id.starts_with("path:") {
        return false;
    }
    is_contributable_for_ops(topic_id, label)
}

/// Ops-facing contribution: readable sanitized label required; opaque ids alone are not enough.
pub fn is_contributable_for_ops(topic_id: &str, label: &str) -> bool {
    if !is_contributable_topic_key(topic_id, label) {
        return false;
    }
    let label_redacted = sanitize_text(label);
    if label_redacted.is_empty() || contains_residual_pii(&label_redacted) {
        return false;
    }
    if looks_like_opaque_token(&label_redacted) {
        return false;
    }
    if is_opaque_local_topic_id(topic_id) && label_redacted.split_whitespace().count() <= 1 {
        // Single-token declared rows must still look like words, not hex ids.
        let word = label_redacted.trim();
        if looks_like_opaque_token(word) || word.len() < 2 {
            return false;
        }
    }
    true
}

/// Human-readable key for cohort stats (never upload raw `interest:<hex>`).
pub fn readable_topic_key(topic_id: &str, label_redacted: &str) -> String {
    let id = topic_id.trim().to_ascii_lowercase();
    if id.starts_with("lang:") || id.starts_with("tech:") || id.starts_with("topic:") {
        return id;
    }
    format!("topic:{}", slugify_name(label_redacted))
}

pub fn sanitize_summary(summary: &str) -> Option<String> {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        return None;
    }
    let out = sanitize_text(trimmed);
    if out.is_empty() || contains_residual_pii(&out) {
        return None;
    }
    Some(truncate_chars(&out, 280))
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

pub fn filter_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty() && t.len() <= 32 && !POI_BLOCKLIST.contains(&t.as_str()))
        .take(8)
        .collect()
}

pub fn taxonomy_hints_for(readable_key: &str) -> Vec<String> {
    let key = readable_key.trim().to_ascii_lowercase();
    let hint = match key.as_str() {
        "lang:rust" => Some("software.backend.rust"),
        "lang:python" => Some("software.backend.python"),
        "lang:typescript" | "lang:javascript" => Some("software.frontend.web"),
        "lang:go" => Some("software.backend.go"),
        "lang:java" | "lang:kotlin" => Some("software.backend.jvm"),
        "tech:kubernetes" | "tech:docker" => Some("software.devops.containers"),
        "tech:sqlite" => Some("software.data.sql"),
        _ if key.starts_with("lang:") => Some("software.general"),
        _ if key.starts_with("tech:") => Some("software.general"),
        _ => None,
    };
    hint.map(|h| vec![h.to_string()]).unwrap_or_default()
}

pub fn slugify_name(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    let slug: String = lower
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect();
    let collapsed = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.len() > 64 {
        collapsed.chars().take(64).collect()
    } else if collapsed.is_empty() {
        "skill".to_string()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_email_and_path() {
        let raw = "Contact user@example.com at C:\\Users\\alice\\proj";
        let out = sanitize_text(raw);
        assert!(!out.contains("user@example.com"));
        assert!(!out.contains("C:\\Users\\alice"));
    }

    #[test]
    fn opaque_interest_id_not_readable_key() {
        assert!(is_opaque_local_topic_id("interest:0062d40fb666492a"));
        assert!(!is_opaque_local_topic_id("lang:rust"));
        let key = readable_topic_key("interest:0062d40fb666492a", "Beijing dialect");
        assert_eq!(key, "topic:beijing-dialect");
    }

    #[test]
    fn persistable_local_rejects_keyword_and_path() {
        assert!(!is_persistable_local_poi("keyword:database", "keyword: database"));
        assert!(!is_persistable_local_poi("path:crates/foo", "path: crates/foo"));
        assert!(is_persistable_local_poi("lang:rust", "language: rust"));
    }

    #[test]
    fn contributable_for_ops_requires_readable_label() {
        assert!(!is_contributable_for_ops(
            "interest:0062d40fb666492a",
            "0062d40fb666492a"
        ));
        assert!(is_contributable_for_ops("interest:abc", "Beijing dialect"));
        assert!(is_contributable_for_ops("lang:rust", "Rust"));
    }
}
