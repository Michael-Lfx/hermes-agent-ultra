//! Golden tests for sanitizer and payload building.



use std::fs;

use std::path::Path;



use hermes_insights::sanitize::{is_contributable_for_ops, sanitize_text};

use hermes_insights::skill::{build_skill_pattern, SkillChangeKind, SkillPatternOptions};

use hermes_insights::{build_interest_fingerprint, InterestTopicInput};



const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");



#[test]

fn golden_skill_with_pii_strips_sensitive_content() {

    let raw = fs::read_to_string(Path::new(FIXTURES).join("skill_with_pii.md")).unwrap();

    let redacted = sanitize_text(&raw);

    assert!(!redacted.contains("user@example.com"));

    assert!(!redacted.contains("sk-live-secretkey"));

    assert!(!redacted.contains("C:\\Users\\alice"));

}



#[test]

fn golden_interest_excludes_path_topics_and_opaque_ids() {

    let topics = vec![

        InterestTopicInput {

            id: "path:crates/hermes-agent".into(),

            label: "crates/hermes-agent".into(),

            summary: String::new(),

            weight: 0.9,

            evidence_count: 4,

            tags: vec![],

        },

        InterestTopicInput {

            id: "interest:0062d40fb666492a".into(),

            label: "0062d40fb666492a".into(),

            summary: String::new(),

            weight: 0.8,

            evidence_count: 3,

            tags: vec![],

        },

        InterestTopicInput {

            id: "lang:rust".into(),

            label: "Rust".into(),

            summary: "Systems programming".into(),

            weight: 0.8,

            evidence_count: 3,

            tags: vec![],

        },

    ];

    assert!(!is_contributable_for_ops("path:crates/hermes-agent", "crates/hermes-agent"));

    assert!(!is_contributable_for_ops(

        "interest:0062d40fb666492a",

        "0062d40fb666492a"

    ));

    let fp = build_interest_fingerprint(&topics).unwrap();

    assert_eq!(fp.topics.len(), 1);

    assert_eq!(fp.topics[0].topic_key, "lang:rust");

    assert_eq!(fp.topics[0].label_redacted, "Rust");

    assert_eq!(fp.co_topics, vec!["Rust".to_string()]);

}



#[test]

fn golden_skill_pattern_from_fixture_dir() {

    let tmp = tempfile::tempdir().unwrap();

    let skills_root = tmp.path().join("skills");

    let skill_dir = skills_root.join("demo");

    fs::create_dir_all(&skill_dir).unwrap();

    fs::copy(

        Path::new(FIXTURES).join("skill_with_pii.md"),

        skill_dir.join("SKILL.md"),

    )

    .unwrap();

    let opts = SkillPatternOptions::from_change_kind(SkillChangeKind::Agent);

    let pattern = build_skill_pattern(&skill_dir, &skills_root, &opts);

    assert!(pattern.is_some());

    let p = pattern.unwrap();

    assert!(!p.description_redacted.contains("user@example.com"));

    assert!(!p.display_name.is_empty());

    assert!(p.redacted_body.is_some());

}


