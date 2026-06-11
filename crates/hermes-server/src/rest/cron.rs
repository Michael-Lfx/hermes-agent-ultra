use axum::{extract::{Path, State}, Json};
use serde_json::json;
use std::sync::Arc;

use hermes_cron::{JobPersistence, JobStatus};

use crate::{error::AppError, state::AppState};

/// Helper to get or create file persistence for cron jobs.
fn cron_persistence(state: &AppState) -> hermes_cron::FileJobPersistence {
    hermes_cron::FileJobPersistence::with_dir(state.cron_data_dir.clone())
}

/// GET /api/cron/jobs - List all cron jobs
pub async fn list_jobs(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = cron_persistence(&state);
    let jobs = persistence.load_jobs().await
        .map_err(|e| AppError::Internal(format!("Failed to load cron jobs: {}", e)))?;
    
    Ok(Json(json!({
        "jobs": jobs.into_iter().map(|job| {
            json!({
                "id": job.id,
                "name": job.name,
                "schedule": job.schedule,
                "prompt": job.prompt,
                "status": job.status.to_string(),
            })
        }).collect::<Vec<_>>()
    })))
}

/// GET /api/cron/jobs/{id} - Get a cron job
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = cron_persistence(&state);
    let jobs = persistence.load_jobs().await
        .map_err(|e| AppError::Internal(format!("Failed to load cron jobs: {}", e)))?;
    
    let job = jobs.into_iter()
        .find(|j| j.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Cron job '{}' not found", id)))?;
    
    Ok(Json(json!({
        "id": job.id,
        "name": job.name,
        "schedule": job.schedule,
        "prompt": job.prompt,
        "status": job.status.to_string(),
    })))
}

/// GET /api/cron/jobs/{id}/runs - Get job run history
pub async fn get_job_runs(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: Implement run history tracking
    Ok(Json(json!({
        "job_id": id,
        "runs": []
    })))
}

/// POST /api/cron/jobs - Create a cron job
pub async fn create_job(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let name = payload["name"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing job name".to_string()))?;
    let schedule = payload["schedule"].as_str()
        .ok_or_else(|| AppError::BadRequest("Missing schedule".to_string()))?;
    let prompt = payload["prompt"].as_str()
        .unwrap_or("");
    
    let mut job = hermes_cron::CronJob::new(schedule, prompt);
    job.name = Some(name.to_string());
    
    let persistence = cron_persistence(&state);
    persistence.save_job(&job).await
        .map_err(|e| AppError::Internal(format!("Failed to save cron job: {}", e)))?;
    
    Ok(Json(json!({
        "status": "ok",
        "id": job.id,
        "name": name,
        "schedule": schedule,
    })))
}

/// PUT /api/cron/jobs/{id} - Update a cron job
pub async fn update_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = cron_persistence(&state);
    let mut jobs = persistence.load_jobs().await
        .map_err(|e| AppError::Internal(format!("Failed to load cron jobs: {}", e)))?;
    
    let job = jobs.iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Cron job '{}' not found", id)))?;
    
    if let Some(name) = payload["name"].as_str() {
        job.name = Some(name.to_string());
    }
    if let Some(schedule) = payload["schedule"].as_str() {
        job.schedule = schedule.to_string();
    }
    if let Some(prompt) = payload["prompt"].as_str() {
        job.prompt = prompt.to_string();
    }
    
    persistence.save_jobs(&jobs).await
        .map_err(|e| AppError::Internal(format!("Failed to save cron jobs: {}", e)))?;
    
    Ok(Json(json!({
        "status": "ok",
        "id": id,
    })))
}

/// POST /api/cron/jobs/{id}/pause - Pause a cron job
pub async fn pause_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = cron_persistence(&state);
    let mut jobs = persistence.load_jobs().await
        .map_err(|e| AppError::Internal(format!("Failed to load cron jobs: {}", e)))?;
    
    let job = jobs.iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Cron job '{}' not found", id)))?;
    
    job.status = JobStatus::Paused;
    
    persistence.save_jobs(&jobs).await
        .map_err(|e| AppError::Internal(format!("Failed to save cron jobs: {}", e)))?;
    
    Ok(Json(json!({
        "status": "ok",
        "id": id,
        "action": "paused",
    })))
}

/// POST /api/cron/jobs/{id}/resume - Resume a cron job
pub async fn resume_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = cron_persistence(&state);
    let mut jobs = persistence.load_jobs().await
        .map_err(|e| AppError::Internal(format!("Failed to load cron jobs: {}", e)))?;
    
    let job = jobs.iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Cron job '{}' not found", id)))?;
    
    job.status = JobStatus::Active;
    
    persistence.save_jobs(&jobs).await
        .map_err(|e| AppError::Internal(format!("Failed to save cron jobs: {}", e)))?;
    
    Ok(Json(json!({
        "status": "ok",
        "id": id,
        "action": "resumed",
    })))
}

/// POST /api/cron/jobs/{id}/trigger - Manually trigger a cron job
pub async fn trigger_job(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    tracing::warn!(job_id = %id, "Manual trigger not yet implemented (no CronRunner)");
    
    Ok(Json(json!({
        "status": "ok",
        "id": id,
        "action": "triggered",
        "note": "Execution not yet implemented",
    })))
}

/// DELETE /api/cron/jobs/{id} - Delete a cron job
pub async fn delete_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let persistence = cron_persistence(&state);
    persistence.delete_job(&id).await
        .map_err(|e| AppError::Internal(format!("Failed to delete cron job: {}", e)))?;
    
    Ok(Json(json!({
        "status": "ok",
        "id": id,
        "action": "deleted",
    })))
}
