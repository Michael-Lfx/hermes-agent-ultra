use axum::{
    middleware,
    routing::{delete, get, patch, post, put},
    Router,
};
use std::net::SocketAddr;
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing::info;

use crate::{
    error::AppError,
    state::AppState,
};

/// Build the axum router with all routes and middleware.
pub fn router(state: AppState) -> Router {
    Router::new()
        // System endpoints
        .route("/api/status", get(crate::rest::status::handler))
        .route("/api/system/stats", get(crate::rest::status::system_stats))
        
        // Config endpoints
        .route("/api/config", get(crate::rest::config::get_config))
        .route("/api/config", put(crate::rest::config::put_config))
        .route("/api/config/defaults", get(crate::rest::config::get_defaults))
        .route("/api/config/schema", get(crate::rest::config::get_schema))
        .route("/api/config/raw", get(crate::rest::config::get_raw_config))
        .route("/api/config/raw", put(crate::rest::config::put_raw_config))
        
        // Sessions endpoints
        .route("/api/sessions", get(crate::rest::sessions::list_sessions))
        .route("/api/sessions/search", get(crate::rest::sessions::search_sessions))
        .route("/api/sessions/{id}", get(crate::rest::sessions::get_session))
        .route("/api/sessions/{id}/messages", get(crate::rest::sessions::get_session_messages))
        .route("/api/sessions/{id}", delete(crate::rest::sessions::delete_session))
        .route("/api/sessions/{id}", patch(crate::rest::sessions::update_session))
        .route("/api/sessions/{id}/export", get(crate::rest::sessions::export_session))
        .route("/api/sessions/prune", post(crate::rest::sessions::prune_sessions))
        .route("/api/sessions/stats", get(crate::rest::sessions::session_stats))
        .route("/api/sessions/empty/count", get(crate::rest::sessions::count_empty_sessions))
        .route("/api/sessions/empty", delete(crate::rest::sessions::delete_empty_sessions))
        .route("/api/sessions/bulk-delete", post(crate::rest::sessions::bulk_delete_sessions))
        
        // Env endpoints
        .route("/api/env", get(crate::rest::env::list_env))
        .route("/api/env", put(crate::rest::env::set_env_var))
        .route("/api/env", delete(crate::rest::env::delete_env_var))
        .route("/api/env/reveal", post(crate::rest::env::reveal_env_var))
        
        // Model endpoints
        .route("/api/model/info", get(crate::rest::models::get_model_info))
        .route("/api/model/options", get(crate::rest::models::get_model_options))
        .route("/api/model/set", post(crate::rest::models::set_model))
        .route("/api/model/recommended-default", get(crate::rest::models::get_recommended_default))
        .route("/api/model/auxiliary", get(crate::rest::models::get_auxiliary_models))
        
        // Ops endpoints
        .route("/api/ops/doctor", post(crate::rest::ops::run_doctor))
        .route("/api/ops/backup", post(crate::rest::ops::create_backup))
        .route("/api/ops/import", post(crate::rest::ops::import_backup))
        .route("/api/ops/dump", post(crate::rest::ops::dump_state))
        .route("/api/ops/logs", get(crate::rest::ops::get_logs))
        
        // Logs alias (Desktop compatibility)
        .route("/api/logs", get(crate::rest::ops::get_logs))
        
        // Events endpoints
        .route("/api/pub", post(crate::rest::events::publish_event))
        .route("/api/events", get(crate::rest::events::subscribe_events))
        
        // WebSocket endpoint
        .route("/api/ws", get(crate::ws::handler::ws_handler))
        
        // Memory endpoints
        .route("/api/memory", get(crate::rest::memory::get_memory_status))
        .route("/api/memory/provider", put(crate::rest::memory::set_memory_provider))
        .route("/api/memory/reset", post(crate::rest::memory::reset_memory))
        
        // Profile endpoints
        .route("/api/profiles/sessions", get(crate::rest::profiles::get_profiles_sessions))
        .route("/api/profiles", get(crate::rest::profiles::list_profiles))
        .route("/api/profiles", post(crate::rest::profiles::create_profile))
        .route("/api/profiles/{name}", patch(crate::rest::profiles::rename_profile))
        .route("/api/profiles/{name}", delete(crate::rest::profiles::delete_profile))
        .route("/api/profiles/active", get(crate::rest::profiles::get_active_profile))
        .route("/api/profiles/active", post(crate::rest::profiles::set_active_profile))
        .route("/api/profiles/{name}/soul", get(crate::rest::profiles::get_profile_soul))
        .route("/api/profiles/{name}/soul", put(crate::rest::profiles::put_profile_soul))
        .route("/api/profiles/{name}/setup-command", get(crate::rest::profiles::get_profile_setup_command))
        
        // Skills endpoints
        .route("/api/skills", get(crate::rest::skills::get_skills))
        .route("/api/skills/toggle", put(crate::rest::skills::toggle_skill))
        
        // Toolsets endpoints
        .route("/api/tools/toolsets", get(crate::rest::toolsets::list_toolsets))
        .route("/api/tools/toolsets/{name}", put(crate::rest::toolsets::toggle_toolset))
        .route("/api/tools/toolsets/{name}/config", get(crate::rest::toolsets::get_toolset_config))
        .route("/api/tools/toolsets/{name}/provider", put(crate::rest::toolsets::set_toolset_provider))
        .route("/api/tools/toolsets/{name}/post-setup", post(crate::rest::toolsets::run_post_setup))
        
        // Cron endpoints
        .route("/api/cron/jobs", get(crate::rest::cron::list_jobs))
        .route("/api/cron/jobs", post(crate::rest::cron::create_job))
        .route("/api/cron/jobs/{id}", get(crate::rest::cron::get_job))
        .route("/api/cron/jobs/{id}", put(crate::rest::cron::update_job))
        .route("/api/cron/jobs/{id}", delete(crate::rest::cron::delete_job))
        .route("/api/cron/jobs/{id}/runs", get(crate::rest::cron::get_job_runs))
        .route("/api/cron/jobs/{id}/pause", post(crate::rest::cron::pause_job))
        .route("/api/cron/jobs/{id}/resume", post(crate::rest::cron::resume_job))
        .route("/api/cron/jobs/{id}/trigger", post(crate::rest::cron::trigger_job))
        
        // Messaging endpoints
        .route("/api/messaging/platforms", get(crate::rest::messaging::list_platforms))
        .route("/api/messaging/platforms/{id}", put(crate::rest::messaging::update_platform))
        .route("/api/messaging/platforms/{id}/test", post(crate::rest::messaging::test_platform))
        
        // Gateway control endpoints
        .route("/api/gateway/start", post(crate::rest::gateway::start_gateway))
        .route("/api/gateway/stop", post(crate::rest::gateway::stop_gateway))
        .route("/api/gateway/restart", post(crate::rest::gateway::restart_gateway))
        
        // Audio endpoints
        .route("/api/audio/transcribe", post(crate::rest::audio::transcribe_audio))
        .route("/api/audio/speak", post(crate::rest::audio::speak_text))
        .route("/api/audio/elevenlabs/voices", get(crate::rest::audio::list_voices))
        
        // Health check (compatibility with hermes-http)
        .route("/health", get(crate::rest::status::health))
        
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024)) // 2 MiB
        .layer(middleware::from_fn_with_state(state.clone(), crate::middleware::request_guard))
        .with_state(state)
}

/// Run the HTTP server on the given address.
pub async fn run_server(
    addr: SocketAddr,
    state: AppState,
) -> Result<(), AppError> {
    let app = router(state);
    
    info!("hermes-server listening on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Internal(format!("failed to bind: {}", e)))?;
    
    axum::serve(listener, app)
        .await
        .map_err(|e| AppError::Internal(format!("server error: {}", e)))?;
    
    Ok(())
}
