pub mod core;
pub mod error;
pub mod middleware;
pub mod rest;
pub mod rpc;
pub mod server;
pub mod state;
pub mod ws;

use std::net::SocketAddr;

use hermes_config::{load_config, GatewayConfig};
use tracing::info;

use crate::{
    error::AppError,
    server::run_server,
    state::AppState,
};

/// Run the hermes-server on the given address.
/// 
/// # Example
/// ```no_run
/// # use std::net::SocketAddr;
/// # #[tokio::main]
/// # async fn main() {
/// #     let addr = "127.0.0.1:9119".parse::<SocketAddr>().unwrap();
/// #     hermes_server::run(addr).await.unwrap();
/// # }
/// ```
pub async fn run(addr: SocketAddr) -> Result<(), AppError> {
    // Load configuration
    let config = load_config(None)
        .map_err(|e| AppError::Config(format!("failed to load config: {}", e)))?;
    
    let hermes_home = hermes_config::hermes_home();
    info!("HERMES_HOME: {}", hermes_home.display());
    
    let state = AppState::new(config, hermes_home);
    
    run_server(addr, state).await
}

/// Run the server with an explicit configuration.
pub async fn run_with_config(addr: SocketAddr, config: GatewayConfig) -> Result<(), AppError> {
    let hermes_home = hermes_config::hermes_home();
    let state = AppState::new(config, hermes_home);
    
    run_server(addr, state).await
}

/// Run the server with a specific profile.
pub async fn run_with_profile(addr: SocketAddr, profile: &str) -> Result<(), AppError> {
    // Load configuration
    let config = load_config(None)
        .map_err(|e| AppError::Config(format!("failed to load config: {}", e)))?;
    
    let hermes_home = hermes_config::hermes_home();
    info!("HERMES_HOME: {}", hermes_home.display());
    info!("Profile: {}", profile);
    
    let state = AppState::new(config, hermes_home);
    
    // Set active profile
    if profile != "default" {
        let profile_home = state.profile_home(Some(profile));
        if !profile_home.exists() {
            std::fs::create_dir_all(&profile_home)
                .map_err(|e| AppError::Internal(format!("create profile dir: {}", e)))?;
            info!("Created profile directory: {}", profile_home.display());
        }
        *state.active_profile.write().await = profile.to_string();
    }
    
    run_server(addr, state).await
}
