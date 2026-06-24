//! Durable workflow run manifest (`manifest.json` per run).

use chrono::Utc;
use serde::Serialize;

use super::store::WorkflowRunRecord;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowManifest {
    pub run_id: String,
    pub workflow_id: String,
    pub status: String,
    pub inputs: serde_json::Value,
    pub step_ids: Vec<String>,
    pub artifacts: Vec<serde_json::Value>,
    pub error: Option<String>,
    pub updated_at: String,
}

impl WorkflowManifest {
    pub fn from_record(record: &WorkflowRunRecord) -> Self {
        Self {
            run_id: record.run_id.clone(),
            workflow_id: record.workflow_id.clone(),
            status: format!("{:?}", record.status).to_ascii_lowercase(),
            inputs: record.inputs.clone(),
            step_ids: record.step_outputs.keys().cloned().collect(),
            artifacts: record.artifacts.clone(),
            error: record.error.clone(),
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::store::{WorkflowRunRecord, WorkflowRunStatus};

    #[test]
    fn manifest_reflects_record_fields() {
        let record = WorkflowRunRecord {
            run_id: "run-1".into(),
            workflow_id: "txt2img".into(),
            status: WorkflowRunStatus::Succeeded,
            inputs: serde_json::json!({"prompt": "cat"}),
            current_step: None,
            step_outputs: [("image".into(), serde_json::json!({"ok": true}))]
                .into_iter()
                .collect(),
            artifacts: vec![serde_json::json!({"kind": "image"})],
            error: None,
        };
        let manifest = WorkflowManifest::from_record(&record);
        assert_eq!(manifest.run_id, "run-1");
        assert_eq!(manifest.status, "succeeded");
        assert_eq!(manifest.step_ids.len(), 1);
        assert_eq!(manifest.artifacts.len(), 1);
    }
}
