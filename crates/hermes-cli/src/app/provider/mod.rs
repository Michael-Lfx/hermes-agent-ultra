mod agent_config;
mod api_keys;
mod build;
mod cache;
mod names;
mod no_backend;
mod resolve;
mod runtime_cli;
mod tool_bridge;
mod urls;

pub use agent_config::build_agent_config;
pub use api_keys::provider_api_key_from_env;
pub use build::build_provider;
pub use tool_bridge::{async_tool_dispatch_for, bridge_tool_registry};

pub(crate) use api_keys::allow_no_api_key;
pub(crate) use cache::{clear_provider_cache, provider_cache, provider_cache_key};
pub(crate) use no_backend::NoBackendProvider;

pub(super) use names::normalize_runtime_provider_name;
pub(super) use resolve::{resolve_provider_and_model, resolve_startup_model};
pub(super) use runtime_cli::{
    apply_cli_runtime_overrides, default_mouse_enabled, default_rtk_raw_mode,
    sync_runtime_model_env,
};
