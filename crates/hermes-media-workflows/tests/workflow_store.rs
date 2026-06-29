//! Integration tests for workflow persistence and async kickoff.

use std::sync::Arc;

use hermes_config::{MediaGenConfig, ServerConfig};
use hermes_media_workflows::backends::FlowyMediaServices;
use hermes_media_workflows::workflows::definition::WorkflowPlan;
use hermes_media_workflows::workflows::runner::WorkflowRunner;
use hermes_media_workflows::workflows::store::{WorkflowRunStatus, WorkflowRunStore};
use hermes_media_workflows::workflows::templates::{builtin_template, default_template_inputs};
use hermes_server_client::{FlowyApiClient, ServerSession};

struct TestHarness {
    services: FlowyMediaServices,
    _home: tempfile::TempDir,
}

fn test_harness(media: MediaGenConfig) -> TestHarness {
    let home = tempfile::tempdir().expect("tmpdir");
    let server = ServerConfig {
        enabled: true,
        base_url: "http://127.0.0.1:9".into(),
        ..Default::default()
    };
    let api = FlowyApiClient::new(&server).expect("api client");
    let services = FlowyMediaServices {
        api: Arc::new(api),
        session: ServerSession::from_config(&server, home.path()),
        media,
        server,
        hermes_home: home.path().to_path_buf(),
    };
    TestHarness {
        services,
        _home: home,
    }
}

#[test]
fn store_writes_manifest_on_save() {
    let root = tempfile::tempdir().expect("tmpdir");
    let store = WorkflowRunStore::with_root(root.path().to_path_buf());
    let mut record = store.create_run("txt2img", serde_json::json!({"prompt": "sunset"}));
    record.status = WorkflowRunStatus::Running;
    store.save(&record);

    let manifest_path = root.path().join(&record.run_id).join("manifest.json");
    assert!(manifest_path.exists(), "manifest.json should be written");
    let text = std::fs::read_to_string(manifest_path).expect("read manifest");
    assert!(text.contains("txt2img"));
    assert!(text.contains("running"));
}

#[tokio::test]
async fn spawn_plan_returns_run_id_immediately() {
    let mut media = MediaGenConfig::default();
    media.provider = "flowy".into();
    media.workflows.async_execution = true;
    media.workflows.llm_prompt_refine = false;
    media.workflows.check_credits = false;

    let harness = test_harness(media);
    let root = tempfile::tempdir().expect("tmpdir");
    let store = Arc::new(WorkflowRunStore::with_root(root.path().to_path_buf()));
    let runner = Arc::new(WorkflowRunner::new(harness.services, store.clone()));

    let def = builtin_template("simple_txt2img").expect("template");
    let inputs = default_template_inputs("simple_txt2img", "a red balloon", None);
    let plan = WorkflowPlan::from_definition(&def, inputs);

    let run_id = runner.spawn_plan(plan).expect("spawn");
    assert!(!run_id.is_empty());

    let record = store.get(&run_id).expect("record");
    assert_eq!(record.status, WorkflowRunStatus::Running);

    // Allow background task to fail (no real server); status should settle to failed.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let final_record = store.get(&run_id).expect("record after spawn");
    assert!(
        final_record.status == WorkflowRunStatus::Failed
            || final_record.status == WorkflowRunStatus::Running,
        "unexpected status: {:?}",
        final_record.status
    );
}
