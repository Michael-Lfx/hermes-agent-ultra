use hermes_config::GatewayConfig;

pub(crate) fn resolve_provider_and_model(config: &GatewayConfig, model: &str) -> (String, String) {
    let trimmed = model.trim();
    if let Some((provider, model_name)) = trimmed.split_once(':') {
        return (provider.trim().to_string(), model_name.trim().to_string());
    }

    if let Some((provider, _)) = config.llm_providers.iter().find(|(_, cfg)| {
        cfg.model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .is_some_and(|m| m == trimmed)
    }) {
        return (provider.to_string(), trimmed.to_string());
    }

    if config.llm_providers.len() == 1 {
        if let Some((provider, _)) = config.llm_providers.iter().next() {
            return (provider.to_string(), trimmed.to_string());
        }
    }

    ("openai".to_string(), trimmed.to_string())
}

pub(crate) fn resolve_startup_model(config: &GatewayConfig, configured_model: &str) -> String {
    let raw = configured_model.trim();
    if raw.is_empty() {
        return "gpt-4o".to_string();
    }
    if raw.contains(':') {
        return raw.to_string();
    }

    // If config.model is a provider slug (e.g. "nous"), prefer that provider's
    // configured runtime model instead of sending the bare slug as a model id.
    if let Some((provider, provider_cfg)) = config
        .llm_providers
        .iter()
        .find(|(provider, _)| provider.eq_ignore_ascii_case(raw))
    {
        if let Some(runtime_model) = provider_cfg
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .filter(|m| !m.eq_ignore_ascii_case(provider))
        {
            if runtime_model.contains(':') {
                return runtime_model.to_string();
            }
            return format!("{provider}:{runtime_model}");
        }
    }

    raw.to_string()
}
