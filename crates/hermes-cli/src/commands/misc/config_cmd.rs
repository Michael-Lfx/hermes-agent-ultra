//! `/config` slash command handler.

use std::sync::Arc;

use crate::commands::{CommandResult, emit_command_output};
use hermes_core::AgentError;

pub(crate) fn handle_config_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    if args.is_empty() {
        let config_json = serde_json::to_string_pretty(&*host.config())
            .unwrap_or_else(|e| format!("<serialization error: {}>", e));
        emit_command_output(host, config_json);
    } else {
        match args[0] {
            "get" => {
                if args.len() < 2 {
                    emit_command_output(host, "Usage: /config get <key>");
                } else {
                    let key = args[1];
                    let value = get_config_value(host, key);
                    match value {
                        Some(v) => emit_command_output(host, format!("{} = {}", key, v)),
                        None => emit_command_output(
                            host,
                            format!("Key '{}' not found in configuration.", key),
                        ),
                    }
                }
            }
            "set" => {
                if args.len() < 3 {
                    emit_command_output(host, "Usage: /config set <key> <value>");
                } else {
                    let key = args[1];
                    let value = args[2..].join(" ");
                    if set_config_value(host, key, &value) {
                        emit_command_output(host, format!("Set {} = {}", key, value));
                    } else {
                        emit_command_output(host, format!("Unknown configuration key: {}", key));
                    }
                }
            }
            _ => {
                emit_command_output(
                    host,
                    format!("Unknown config action '{}'. Use 'get' or 'set'.", args[0]),
                );
            }
        }
    }
    Ok(CommandResult::Handled)
}

fn get_config_value(host: &impl crate::app::ModelRuntime, key: &str) -> Option<String> {
    match key {
        "model" => host.config().model.clone(),
        "personality" => host.config().personality.clone(),
        "max_turns" => Some(host.config().max_turns.to_string()),
        "system_prompt" => host.config().system_prompt.clone(),
        _ => None,
    }
}

fn set_config_value(host: &mut impl crate::app::SlashCommandHost, key: &str, value: &str) -> bool {
    match key {
        "model" => {
            host.set_config(Arc::new({
                let mut cfg = host.config().as_ref().clone();
                cfg.model = Some(value.to_string());
                cfg
            }));
            host.switch_model(value);
            true
        }
        "personality" => {
            host.set_config(Arc::new({
                let mut cfg = host.config().as_ref().clone();
                cfg.personality = Some(value.to_string());
                cfg
            }));
            host.switch_personality(value);
            true
        }
        "max_turns" => {
            if let Ok(turns) = value.parse::<u32>() {
                host.set_config(Arc::new({
                    let mut cfg = host.config().as_ref().clone();
                    cfg.max_turns = turns;
                    cfg
                }));
                true
            } else {
                false
            }
        }
        _ => false,
    }
}
