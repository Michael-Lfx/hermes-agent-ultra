//! Local user interest (POI) topic store and memory provider integration.

mod declared;
mod extract;
mod ingest;
mod llm;
mod plugin;
mod store;
mod topic_id;

pub use extract::{
    extract_signals_from_messages, extract_signals_from_text, filter_poi_signals,
    is_rejected_poi_topic,
};
pub use ingest::{
    ingest_user_message, is_poi_synthetic_user_text, spawn_session_end_ingest,
};
pub use plugin::InterestMemoryPlugin;
pub use store::{InterestSignal, InterestStore, InterestTopic, load_interest_snapshot};

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Open the shared interest DB (independent of `skip_memory`).
pub fn open_interest_store(
    hermes_home: &str,
    config: &hermes_config::InterestConfig,
) -> Option<Arc<Mutex<InterestStore>>> {
    if !config.enabled {
        return None;
    }
    let db_path = PathBuf::from(hermes_home).join("interest.db");
    InterestStore::open(&db_path, config.clone())
        .ok()
        .map(|store| Arc::new(Mutex::new(store)))
}
