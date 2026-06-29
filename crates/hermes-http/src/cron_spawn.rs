use hermes_tasks::CronJob;
use hermes_tasks::types::DeviceId;
use tracing::warn;

use crate::HttpServerState;

pub async fn spawn_due_cron_job(state: HttpServerState, job: CronJob) {
    let Some(tasks) = state.tasks.clone() else {
        return;
    };
    let vertical = job.vertical.clone();
    let title = job.title.clone();
    let instruction = job.prompt_template.clone();
    let owner = job.owner_user_id;
    let device = DeviceId::new();

    match tasks
        .runtime
        .create_and_run(owner, device, title, vertical, &instruction)
        .await
    {
        Ok((task, event)) => {
            crate::task_agent::spawn_task_agent_run(
                state,
                task,
                instruction,
                event.turn_id,
                tasks.cancellation.clone(),
            );
        }
        Err(err) => warn!(job_id = %job.id, error = %err, "cron task spawn failed"),
    }
}
