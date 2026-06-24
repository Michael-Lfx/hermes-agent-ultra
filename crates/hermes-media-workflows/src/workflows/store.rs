//! File-backed workflow run state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunRecord {
    pub run_id: String,
    pub workflow_id: String,
    pub status: WorkflowRunStatus,
    pub inputs: Value,
    pub current_step: Option<String>,
    #[serde(default)]
    pub step_outputs: HashMap<String, Value>,
    #[serde(default)]
    pub artifacts: Vec<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

pub struct WorkflowRunStore {
    root: PathBuf,
    memory: Arc<RwLock<HashMap<String, WorkflowRunRecord>>>,
}

impl WorkflowRunStore {
    pub fn new() -> Self {
        let root = hermes_config::hermes_home().join("media").join("workflows");
        let _ = std::fs::create_dir_all(&root);
        Self {
            root,
            memory: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn create_run(&self, workflow_id: &str, inputs: Value) -> WorkflowRunRecord {
        let run_id = Uuid::new_v4().to_string();
        let record = WorkflowRunRecord {
            run_id: run_id.clone(),
            workflow_id: workflow_id.to_string(),
            status: WorkflowRunStatus::Pending,
            inputs,
            current_step: None,
            step_outputs: HashMap::new(),
            artifacts: Vec::new(),
            error: None,
        };
        self.save(&record);
        record
    }

    pub fn get(&self, run_id: &str) -> Option<WorkflowRunRecord> {
        if let Ok(guard) = self.memory.read()
            && let Some(rec) = guard.get(run_id)
        {
            return Some(rec.clone());
        }
        let path = self.run_path(run_id);
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(&self, record: &WorkflowRunRecord) {
        if let Ok(mut guard) = self.memory.write() {
            guard.insert(record.run_id.clone(), record.clone());
        }
        let path = self.run_path(&record.run_id);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(record) {
            let _ = std::fs::write(path, json);
        }
    }

    fn run_path(&self, run_id: &str) -> PathBuf {
        self.root.join(run_id).join("state.json")
    }
}

impl Default for WorkflowRunStore {
    fn default() -> Self {
        Self::new()
    }
}
