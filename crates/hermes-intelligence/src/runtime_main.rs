//! Live main runtime provider/model for auxiliary routing (Python `set_runtime_main`).

use std::sync::RwLock;

static RUNTIME_MAIN: RwLock<(String, String)> = RwLock::new((String::new(), String::new()));

/// Record the active main provider/model for this turn.
pub fn set_runtime_main(provider: &str, model: &str) {
    if let Ok(mut slot) = RUNTIME_MAIN.write() {
        *slot = (provider.trim().to_lowercase(), model.trim().to_string());
    }
}

/// Clear the runtime override (e.g. on session end).
pub fn clear_runtime_main() {
    if let Ok(mut slot) = RUNTIME_MAIN.write() {
        *slot = (String::new(), String::new());
    }
}

/// Snapshot of the last `set_runtime_main` values.
pub fn runtime_main_provider_model() -> (String, String) {
    RUNTIME_MAIN
        .read()
        .map(|g| g.clone())
        .unwrap_or_default()
}
