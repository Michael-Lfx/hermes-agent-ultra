//! ACP session state management.
//!
//! Maps ACP sessions to Hermes agent instances with persistence support.
//! Mirrors the Python `acp_adapter/session.py` implementation.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use hermes_agent::session_persistence::SessionPersistence;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// SessionPhase
// ---------------------------------------------------------------------------

/// Lifecycle phase of an ACP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    /// Session created, awaiting first prompt.
    Created,
    /// Session is actively processing a prompt.
    Active,
    /// Session is idle, waiting for the next prompt.
    Idle,
    /// Session completed normally.
    Completed,
    /// Session was cancelled by the client.
    Cancelled,
    /// Session encountered an unrecoverable error.
    Failed,
}

impl Default for SessionPhase {
    fn default() -> Self {
        Self::Created
    }
}

// ---------------------------------------------------------------------------
// SessionState
// ---------------------------------------------------------------------------

/// Tracks per-session state for an ACP-managed Hermes agent.
#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: String,
    pub cwd: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub api_mode: Option<String>,
    pub base_url: Option<String>,
    pub phase: SessionPhase,
    pub history: Vec<Value>,
    pub mode: Option<String>,
    pub config_options: HashMap<String, String>,
    pub created_at: u64,
    pub updated_at: u64,
    /// Total prompt tokens across all turns.
    pub total_prompt_tokens: u64,
    /// Total completion tokens across all turns.
    pub total_completion_tokens: u64,
}

impl SessionState {
    pub fn new(session_id: String, cwd: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            session_id,
            cwd,
            model: None,
            provider: None,
            api_mode: None,
            base_url: None,
            phase: SessionPhase::Created,
            history: Vec::new(),
            mode: None,
            config_options: HashMap::new(),
            created_at: now,
            updated_at: now,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

// ---------------------------------------------------------------------------
// SessionInfo (lightweight view for listing)
// ---------------------------------------------------------------------------

/// Lightweight session info returned by `list_sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub cwd: String,
    pub model: Option<String>,
    pub phase: SessionPhase,
    pub history_len: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

impl From<&SessionState> for SessionInfo {
    fn from(s: &SessionState) -> Self {
        Self {
            session_id: s.session_id.clone(),
            cwd: s.cwd.clone(),
            model: s.model.clone(),
            phase: s.phase,
            history_len: s.history.len(),
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// SessionManager
// ---------------------------------------------------------------------------

/// Thread-safe manager for ACP sessions.
///
/// Sessions are held in-memory for fast access. A persistence callback can be
/// provided to sync state to a database or disk.
pub struct SessionManager {
    sessions: Mutex<HashMap<String, SessionState>>,
    on_persist: Option<Box<dyn Fn(&SessionState) + Send + Sync>>,
    on_restore: Option<Box<dyn Fn(&str) -> Option<SessionState> + Send + Sync>>,
    on_delete: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    on_list: Option<Box<dyn Fn() -> Vec<SessionInfo> + Send + Sync>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            on_persist: None,
            on_restore: None,
            on_delete: None,
            on_list: None,
        }
    }

    /// Create a session manager with ACP session persistence enabled.
    ///
    /// This mirrors Python ACP behavior: session state survives process restart.
    pub fn new_with_default_persistence() -> Self {
        let sp = Arc::new(SessionPersistence::default_home());
        if let Err(err) = sp.ensure_db() {
            tracing::warn!("ACP session persistence initialization failed: {}", err);
        }

        let persist_sp = sp.clone();
        let restore_sp = sp.clone();
        let list_sp = sp.clone();
        let delete_sp = sp;

        Self::new()
            .with_persist_callback(move |state| {
                persist_session_state(&persist_sp, state);
            })
            .with_restore_callback(move |session_id| restore_session_state(&restore_sp, session_id))
            .with_list_callback(move || list_persisted_sessions(&list_sp))
            .with_delete_callback(move |session_id| delete_persisted_session(&delete_sp, session_id))
    }

    /// Set a callback invoked whenever a session is persisted.
    pub fn with_persist_callback(
        mut self,
        cb: impl Fn(&SessionState) + Send + Sync + 'static,
    ) -> Self {
        self.on_persist = Some(Box::new(cb));
        self
    }

    /// Set a callback to restore a session by ID.
    pub fn with_restore_callback(
        mut self,
        cb: impl Fn(&str) -> Option<SessionState> + Send + Sync + 'static,
    ) -> Self {
        self.on_restore = Some(Box::new(cb));
        self
    }

    /// Set a callback to delete a persisted session by ID.
    pub fn with_delete_callback(
        mut self,
        cb: impl Fn(&str) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.on_delete = Some(Box::new(cb));
        self
    }

    /// Set a callback to list persisted sessions.
    pub fn with_list_callback(
        mut self,
        cb: impl Fn() -> Vec<SessionInfo> + Send + Sync + 'static,
    ) -> Self {
        self.on_list = Some(Box::new(cb));
        self
    }

    /// Create a new session with a unique ID.
    pub fn create_session(&self, cwd: &str) -> SessionState {
        let session_id = uuid::Uuid::new_v4().to_string();
        let state = SessionState::new(session_id.clone(), cwd.to_string());
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), state.clone());
        }
        self.persist(&state);
        tracing::info!("Created ACP session {} (cwd={})", session_id, cwd);
        state
    }

    /// Get a session by ID, or `None` if not found.
    pub fn get_session(&self, session_id: &str) -> Option<SessionState> {
        {
            let sessions = self.sessions.lock().unwrap();
            if let Some(state) = sessions.get(session_id) {
                return Some(state.clone());
            }
        }

        let restored = self
            .on_restore
            .as_ref()
            .and_then(|restore| restore(session_id))?;

        let mut sessions = self.sessions.lock().unwrap();
        let entry = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| restored.clone());
        Some(entry.clone())
    }

    /// Update a session's working directory.
    pub fn update_cwd(&self, session_id: &str, cwd: &str) -> Option<SessionState> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state.cwd = cwd.to_string();
            state.touch();
            let cloned = state.clone();
            drop(sessions);
            self.persist(&cloned);
            Some(cloned)
        } else {
            None
        }
    }

    /// Update a session's model identifier.
    pub fn update_model(&self, session_id: &str, model: &str) -> Option<SessionState> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state.model = Some(model.to_string());
            state.touch();
            let cloned = state.clone();
            drop(sessions);
            self.persist(&cloned);
            Some(cloned)
        } else {
            None
        }
    }

    /// Update a session's mode identifier.
    pub fn update_mode(&self, session_id: &str, mode: &str) -> Option<SessionState> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state.mode = Some(mode.to_string());
            state.touch();
            let cloned = state.clone();
            drop(sessions);
            self.persist(&cloned);
            Some(cloned)
        } else {
            None
        }
    }

    /// Set or replace a session config option.
    pub fn set_config_option(
        &self,
        session_id: &str,
        key: &str,
        value: &str,
    ) -> Option<SessionState> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state
                .config_options
                .insert(key.to_string(), value.to_string());
            state.touch();
            let cloned = state.clone();
            drop(sessions);
            self.persist(&cloned);
            Some(cloned)
        } else {
            None
        }
    }

    /// Update a session's phase.
    pub fn set_phase(&self, session_id: &str, phase: SessionPhase) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state.phase = phase;
            state.touch();
        }
    }

    /// Update session history.
    pub fn set_history(&self, session_id: &str, history: Vec<Value>) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state.history = history;
            state.touch();
        }
    }

    /// Fork a session — deep-copy history into a new session.
    pub fn fork_session(&self, session_id: &str, cwd: &str) -> Option<SessionState> {
        let original = self.get_session(session_id)?;
        let new_id = uuid::Uuid::new_v4().to_string();
        let mut new_state = SessionState::new(new_id.clone(), cwd.to_string());
        new_state.model = original.model.clone();
        new_state.history = original.history.clone();
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(new_id.clone(), new_state.clone());
        }
        self.persist(&new_state);
        tracing::info!("Forked ACP session {} -> {}", session_id, new_id);
        Some(new_state)
    }

    /// Remove a session.
    pub fn remove_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        let removed_memory = sessions.remove(session_id).is_some();
        drop(sessions);
        let removed_persisted = self
            .on_delete
            .as_ref()
            .map(|delete| delete(session_id))
            .unwrap_or(false);
        removed_memory || removed_persisted
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        let mut listed: HashMap<String, SessionInfo> = {
            let sessions = self.sessions.lock().unwrap();
            sessions
                .values()
                .map(SessionInfo::from)
                .map(|info| (info.session_id.clone(), info))
                .collect()
        };

        if let Some(list) = &self.on_list {
            for info in list() {
                listed.entry(info.session_id.clone()).or_insert(info);
            }
        }

        listed.into_values().collect()
    }

    /// Persist a session state via the registered callback.
    pub fn save_session(&self, session_id: &str) {
        if let Some(state) = self.get_session(session_id) {
            self.persist(&state);
        }
    }

    /// Accumulate token usage for a session.
    pub fn add_usage(&self, session_id: &str, prompt_tokens: u64, completion_tokens: u64) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state.total_prompt_tokens += prompt_tokens;
            state.total_completion_tokens += completion_tokens;
            state.touch();
        }
    }

    fn persist(&self, state: &SessionState) {
        if let Some(ref cb) = self.on_persist {
            cb(state);
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

const ACP_SESSION_INDEX_KEY: &str = "acp:sessions:index";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSessionState {
    session_id: String,
    cwd: String,
    model: Option<String>,
    provider: Option<String>,
    api_mode: Option<String>,
    base_url: Option<String>,
    phase: SessionPhase,
    history: Vec<Value>,
    mode: Option<String>,
    config_options: HashMap<String, String>,
    created_at: u64,
    updated_at: u64,
    total_prompt_tokens: u64,
    total_completion_tokens: u64,
}

impl From<&SessionState> for PersistedSessionState {
    fn from(s: &SessionState) -> Self {
        Self {
            session_id: s.session_id.clone(),
            cwd: s.cwd.clone(),
            model: s.model.clone(),
            provider: s.provider.clone(),
            api_mode: s.api_mode.clone(),
            base_url: s.base_url.clone(),
            phase: s.phase,
            history: s.history.clone(),
            mode: s.mode.clone(),
            config_options: s.config_options.clone(),
            created_at: s.created_at,
            updated_at: s.updated_at,
            total_prompt_tokens: s.total_prompt_tokens,
            total_completion_tokens: s.total_completion_tokens,
        }
    }
}

impl From<PersistedSessionState> for SessionState {
    fn from(s: PersistedSessionState) -> Self {
        Self {
            session_id: s.session_id,
            cwd: s.cwd,
            model: s.model,
            provider: s.provider,
            api_mode: s.api_mode,
            base_url: s.base_url,
            phase: s.phase,
            history: s.history,
            mode: s.mode,
            config_options: s.config_options,
            created_at: s.created_at,
            updated_at: s.updated_at,
            total_prompt_tokens: s.total_prompt_tokens,
            total_completion_tokens: s.total_completion_tokens,
        }
    }
}

fn session_state_meta_key(session_id: &str) -> String {
    format!("acp:session:{}", session_id)
}

fn read_persisted_session_ids(sp: &SessionPersistence) -> HashSet<String> {
    match sp.get_meta(ACP_SESSION_INDEX_KEY) {
        Ok(Some(raw)) => serde_json::from_str::<Vec<String>>(&raw)
            .unwrap_or_default()
            .into_iter()
            .collect(),
        _ => HashSet::new(),
    }
}

fn write_persisted_session_ids(sp: &SessionPersistence, ids: HashSet<String>) {
    let mut list: Vec<String> = ids.into_iter().collect();
    list.sort();
    if let Ok(raw) = serde_json::to_string(&list) {
        if let Err(err) = sp.set_meta(ACP_SESSION_INDEX_KEY, &raw) {
            tracing::warn!("Failed writing ACP session index: {}", err);
        }
    }
}

fn persist_session_state(sp: &SessionPersistence, state: &SessionState) {
    let persisted = PersistedSessionState::from(state);
    let key = session_state_meta_key(&state.session_id);
    let raw = match serde_json::to_string(&persisted) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                session_id = %state.session_id,
                "Failed serializing ACP session state: {}",
                err
            );
            return;
        }
    };
    if let Err(err) = sp.set_meta(&key, &raw) {
        tracing::warn!(
            session_id = %state.session_id,
            "Failed persisting ACP session state: {}",
            err
        );
        return;
    }
    let mut ids = read_persisted_session_ids(sp);
    ids.insert(state.session_id.clone());
    write_persisted_session_ids(sp, ids);
}

fn restore_session_state(sp: &SessionPersistence, session_id: &str) -> Option<SessionState> {
    let key = session_state_meta_key(session_id);
    let raw = sp.get_meta(&key).ok().flatten()?;
    let parsed = serde_json::from_str::<PersistedSessionState>(&raw).ok()?;
    Some(parsed.into())
}

fn delete_persisted_session(sp: &SessionPersistence, session_id: &str) -> bool {
    let key = session_state_meta_key(session_id);
    let existed = sp
        .get_meta(&key)
        .ok()
        .flatten()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if existed {
        let _ = sp.set_meta(&key, "");
    }
    let mut ids = read_persisted_session_ids(sp);
    let removed_id = ids.remove(session_id);
    if removed_id {
        write_persisted_session_ids(sp, ids);
    }
    existed || removed_id
}

fn list_persisted_sessions(sp: &SessionPersistence) -> Vec<SessionInfo> {
    let mut infos = Vec::new();
    for session_id in read_persisted_session_ids(sp) {
        if let Some(state) = restore_session_state(sp, &session_id) {
            infos.push(SessionInfo::from(&state));
        }
    }
    infos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemoryPersistStore {
        by_id: HashMap<String, SessionState>,
    }

    fn new_test_persistent_manager(
        store: Arc<Mutex<InMemoryPersistStore>>,
    ) -> SessionManager {
        let persist_store = store.clone();
        let restore_store = store.clone();
        let list_store = store.clone();
        let delete_store = store;
        SessionManager::new()
            .with_persist_callback(move |state| {
                if let Ok(mut s) = persist_store.lock() {
                    s.by_id.insert(state.session_id.clone(), state.clone());
                }
            })
            .with_restore_callback(move |sid| {
                restore_store
                    .lock()
                    .ok()
                    .and_then(|s| s.by_id.get(sid).cloned())
            })
            .with_list_callback(move || {
                list_store
                    .lock()
                    .ok()
                    .map(|s| s.by_id.values().map(SessionInfo::from).collect())
                    .unwrap_or_default()
            })
            .with_delete_callback(move |sid| {
                delete_store
                    .lock()
                    .ok()
                    .and_then(|mut s| s.by_id.remove(sid))
                    .is_some()
            })
    }

    #[test]
    fn test_create_and_get_session() {
        let mgr = SessionManager::new();
        let state = mgr.create_session("/tmp");
        assert_eq!(state.cwd, "/tmp");
        assert_eq!(state.phase, SessionPhase::Created);

        let got = mgr.get_session(&state.session_id).unwrap();
        assert_eq!(got.session_id, state.session_id);
    }

    #[test]
    fn test_fork_session() {
        let mgr = SessionManager::new();
        let original = mgr.create_session("/project");
        mgr.set_history(
            &original.session_id,
            vec![serde_json::json!({"role": "user", "content": "hello"})],
        );

        let forked = mgr.fork_session(&original.session_id, "/other").unwrap();
        assert_ne!(forked.session_id, original.session_id);
        assert_eq!(forked.cwd, "/other");
        assert_eq!(forked.history.len(), 1);
    }

    #[test]
    fn test_list_sessions() {
        let mgr = SessionManager::new();
        mgr.create_session("/a");
        mgr.create_session("/b");
        assert_eq!(mgr.list_sessions().len(), 2);
    }

    #[test]
    fn test_remove_session() {
        let mgr = SessionManager::new();
        let state = mgr.create_session("/tmp");
        assert!(mgr.remove_session(&state.session_id));
        assert!(mgr.get_session(&state.session_id).is_none());
    }

    #[test]
    fn test_update_model_mode_and_config_option() {
        let mgr = SessionManager::new();
        let state = mgr.create_session("/tmp");
        let sid = state.session_id;

        mgr.update_model(&sid, "openai:gpt-4o");
        mgr.update_mode(&sid, "code");
        mgr.set_config_option(&sid, "temperature", "0.2");

        let updated = mgr.get_session(&sid).expect("session should exist");
        assert_eq!(updated.model.as_deref(), Some("openai:gpt-4o"));
        assert_eq!(updated.mode.as_deref(), Some("code"));
        assert_eq!(
            updated
                .config_options
                .get("temperature")
                .map(String::as_str),
            Some("0.2")
        );
    }

    #[test]
    fn test_restore_session_from_persistence_when_memory_is_empty() {
        let store = Arc::new(Mutex::new(InMemoryPersistStore::default()));
        let mgr = new_test_persistent_manager(store.clone());
        let state = mgr.create_session("/tmp");
        let sid = state.session_id.clone();
        mgr.set_history(&sid, vec![serde_json::json!({"role":"user","content":"hello"})]);
        mgr.save_session(&sid);

        let mgr2 = new_test_persistent_manager(store);
        let restored = mgr2.get_session(&sid).expect("should restore from persistence");
        assert_eq!(restored.session_id, sid);
        assert_eq!(restored.history.len(), 1);
        assert_eq!(restored.cwd, "/tmp");
    }
}
