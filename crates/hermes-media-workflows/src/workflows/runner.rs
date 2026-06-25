//! Background workflow execution.

use std::sync::Arc;

use hermes_core::{DetachedToolProgressGuard, ToolError};

use super::definition::WorkflowDefinition;
use super::definition::WorkflowPlan;
use super::executor::WorkflowExecutor;
use super::store::{WorkflowRunStatus, WorkflowRunStore};
use crate::backends::FlowyMediaServices;

/// Coordinates sync and async workflow runs.
pub struct WorkflowRunner {
    executor: Arc<WorkflowExecutor>,
    store: Arc<WorkflowRunStore>,
    async_execution: bool,
}

impl WorkflowRunner {
    pub fn new(services: FlowyMediaServices, store: Arc<WorkflowRunStore>) -> Self {
        let async_execution = services.media.workflows.async_execution;
        let max_retries = services.media.workflows.max_retries;
        let executor = Arc::new(WorkflowExecutor::new(
            services,
            Arc::clone(&store),
            max_retries,
        ));
        Self {
            executor,
            store,
            async_execution,
        }
    }

    pub fn async_execution_enabled(&self) -> bool {
        self.async_execution
    }

    pub fn executor(&self) -> Arc<WorkflowExecutor> {
        Arc::clone(&self.executor)
    }

    pub fn store(&self) -> Arc<WorkflowRunStore> {
        Arc::clone(&self.store)
    }

    /// Run synchronously (blocks until complete).
    pub async fn run_plan_sync(
        &self,
        plan: &WorkflowPlan,
    ) -> Result<super::store::WorkflowRunRecord, ToolError> {
        self.executor.run_plan(plan).await
    }

    /// Start async run; returns `run_id` immediately.
    pub fn spawn_plan(self: &Arc<Self>, plan: WorkflowPlan) -> Result<String, ToolError> {
        let def = WorkflowDefinition {
            id: plan.workflow_id.clone(),
            version: plan.template_version,
            description: String::new(),
            inputs: plan.inputs.clone(),
            steps: plan.steps.clone(),
        };
        let mut record = self.store.create_run(&def.id, def.inputs.clone());
        record.status = WorkflowRunStatus::Running;
        self.store.save(&record);
        let run_id = record.run_id.clone();
        let spawn_id = run_id.clone();

        let runner = Arc::clone(self);
        let def = def.clone();
        let detached = DetachedToolProgressGuard::attach(&run_id);
        tokio::spawn(async move {
            let _detached = detached;
            if let Err(err) = runner
                .executor
                .run_definition_existing(&spawn_id, &def)
                .await
            {
                tracing::error!(run_id = %spawn_id, error = %err, "async workflow run failed");
            }
        });
        Ok(run_id)
    }
}
