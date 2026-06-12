use hermes_config::PlatformConfig;
use serde_json::Value;

#[test]
fn reply_prefix_extra_roundtrip() {
    let mut p = PlatformConfig::default();
    p.extra
        .insert("reply_prefix".into(), Value::String("".into()));
    assert_eq!(
        p.extra.get("reply_prefix").and_then(|v| v.as_str()),
        Some("")
    );
}

#[test]
fn policy_fields_in_extra() {
    let mut p = PlatformConfig::default();
    p.extra
        .insert("dm_policy".into(), Value::String("allowlist".into()));
    p.extra.insert(
        "allow_from".into(),
        Value::Array(vec![Value::String("1555".into())]),
    );
    assert_eq!(
        p.extra.get("dm_policy").and_then(|v| v.as_str()),
        Some("allowlist")
    );
}
