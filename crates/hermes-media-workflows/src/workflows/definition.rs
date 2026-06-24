//! Workflow plan / step schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Value,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub on_fail: Option<WorkflowFailAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFailAction {
    #[serde(default)]
    pub retry_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPlan {
    pub workflow_id: String,
    pub template_version: u32,
    pub inputs: Value,
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub estimated_steps: u32,
}

impl WorkflowPlan {
    pub fn from_definition(def: &WorkflowDefinition, inputs: Value) -> Self {
        Self {
            workflow_id: def.id.clone(),
            template_version: def.version,
            inputs,
            estimated_steps: def.steps.len() as u32,
            steps: def.steps.clone(),
        }
    }
}
