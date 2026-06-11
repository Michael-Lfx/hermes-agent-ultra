use hermes_config::GatewayConfig;

use crate::cli::Cli;

use super::names::normalize_runtime_provider_name;
use super::resolve::resolve_provider_and_model;

pub(crate) fn apply_cli_runtime_overrides(config: &mut GatewayConfig, cli: &Cli) {
    if let Some(ref model) = cli.model {
        config.model = Some(model.clone());
    }
    if let Some(ref personality) = cli.personality {
        config.personality = Some(personality.clone());
    }
    if let Some(provider) = cli
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let provider = normalize_runtime_provider_name(provider);
        let existing_model = config.model.as_deref().unwrap_or("gpt-4o").trim();
        let model_name = existing_model
            .split_once(':')
            .map(|(_, name)| name.trim())
            .unwrap_or(existing_model);
        config.model = Some(format!("{provider}:{model_name}"));
    }
}

pub(crate) fn default_mouse_enabled() -> bool {
    match std::env::var("HERMES_TUI_MOUSE") {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        ),
        Err(_) => false,
    }
}

pub(crate) fn default_rtk_raw_mode() -> bool {
    match std::env::var("HERMES_RTK_RAW") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "yes"
        ),
        Err(_) => false,
    }
}

pub(crate) fn sync_runtime_model_env(config: &GatewayConfig, provider_model: &str) {
    let model = provider_model.trim();
    if model.is_empty() {
        return;
    }
    let (provider, _) = resolve_provider_and_model(config, model);
    crate::env_vars::set_var("HERMES_MODEL", model);
    crate::env_vars::set_var("HERMES_INFERENCE_MODEL", model);
    crate::env_vars::set_var("HERMES_INFERENCE_PROVIDER", provider.as_str());
    if std::env::var_os("HERMES_TUI_PROVIDER").is_some() {
        crate::env_vars::set_var("HERMES_TUI_PROVIDER", provider.as_str());
    }
}
