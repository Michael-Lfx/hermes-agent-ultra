//! Adaptive web research loop settings (`agent.web_research` in gateway YAML).

use serde::{Deserialize, Serialize};

/// Runtime caps and planner/evaluator toggles for per-user-message web research.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebResearchConfig {
    #[serde(default = "default_web_research_enabled")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub planner_enabled: bool,
    #[serde(default = "default_true")]
    pub evaluator_enabled: bool,
    #[serde(default = "default_max_search")]
    pub max_search: u32,
    #[serde(default = "default_max_extract")]
    pub max_extract: u32,
    #[serde(default = "default_max_browser")]
    pub max_browser: u32,
    #[serde(default = "default_max_total")]
    pub max_total: u32,
    #[serde(default = "default_fallback_search")]
    pub fallback_search: u32,
    #[serde(default = "default_fallback_extract")]
    pub fallback_extract: u32,
    #[serde(default = "default_fallback_browser")]
    pub fallback_browser: u32,
    #[serde(default = "default_max_consecutive_errors")]
    pub max_consecutive_errors: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner_prompt_path: Option<String>,
}

fn default_web_research_enabled() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_max_search() -> u32 {
    4
}

fn default_max_extract() -> u32 {
    5
}

fn default_max_browser() -> u32 {
    2
}

fn default_max_total() -> u32 {
    8
}

fn default_fallback_search() -> u32 {
    2
}

fn default_fallback_extract() -> u32 {
    5
}

fn default_fallback_browser() -> u32 {
    2
}

fn default_max_consecutive_errors() -> u32 {
    2
}

impl Default for WebResearchConfig {
    fn default() -> Self {
        Self {
            enabled: default_web_research_enabled(),
            planner_enabled: default_true(),
            evaluator_enabled: default_true(),
            max_search: default_max_search(),
            max_extract: default_max_extract(),
            max_browser: default_max_browser(),
            max_total: default_max_total(),
            fallback_search: default_fallback_search(),
            fallback_extract: default_fallback_extract(),
            fallback_browser: default_fallback_browser(),
            max_consecutive_errors: default_max_consecutive_errors(),
            planner_prompt_path: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_research_yaml_deserializes_with_defaults() {
        let cfg: WebResearchConfig = serde_yaml::from_str("enabled: true\n").unwrap();
        assert!(cfg.enabled);
        assert!(cfg.planner_enabled);
        assert_eq!(cfg.max_search, 4);
        assert_eq!(cfg.fallback_search, 2);
    }

    #[test]
    fn web_research_nested_under_agent_block() {
        #[derive(Deserialize)]
        struct Agent {
            #[serde(default)]
            web_research: WebResearchConfig,
        }
        let agent: Agent = serde_yaml::from_str(
            r#"
web_research:
  enabled: false
  max_search: 6
  fallback_search: 1
"#,
        )
        .unwrap();
        assert!(!agent.web_research.enabled);
        assert_eq!(agent.web_research.max_search, 6);
        assert_eq!(agent.web_research.fallback_search, 1);
    }
}
