//! P2-12 slash formatting and autocomplete helpers.

#![cfg(feature = "discord")]

use hermes_core::types::SkillMeta;
use hermes_gateway::platforms::discord::{
    format_slash_command_text, parse_autocomplete_interaction, InteractionOption,
    INTERACTION_TYPE_AUTOCOMPLETE,
};
use hermes_gateway::platforms::discord::slash::{
    format_slash_command_text as slash_format, skill_choices_from_metas,
};

#[test]
fn slash_text_includes_option_args() {
    let text = format_slash_command_text(
        "model",
        &[InteractionOption {
            name: "args".into(),
            value: serde_json::Value::String("gpt-4o".into()),
        }],
    );
    assert_eq!(text, "/model gpt-4o");
}

#[test]
fn slash_format_alias_matches_parse() {
    let opts = vec![InteractionOption {
        name: "args".into(),
        value: serde_json::Value::String("mini".into()),
    }];
    assert_eq!(
        format_slash_command_text("model", &opts),
        slash_format("model", &opts)
    );
}

#[test]
fn parse_autocomplete_interaction_focused_option() {
    let data = serde_json::json!({
        "type": INTERACTION_TYPE_AUTOCOMPLETE,
        "id": "1",
        "token": "tok",
        "data": {
            "name": "skill",
            "options": [
                { "name": "name", "focused": true, "value": "al" }
            ]
        },
        "member": { "user": { "id": "42" }, "roles": [] }
    });
    let ac = parse_autocomplete_interaction(&data).expect("parse");
    assert_eq!(ac.command_name, "skill");
    assert_eq!(ac.focused_option, "name");
    assert_eq!(ac.focused_value, "al");
}

#[test]
fn skill_autocomplete_filters_prefix() {
    let metas = vec![
        SkillMeta {
            name: "alpha".into(),
            category: None,
            description: None,
        },
        SkillMeta {
            name: "beta".into(),
            category: None,
            description: None,
        },
    ];
    let choices = skill_choices_from_metas(&metas, "al");
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].name, "alpha");
}
