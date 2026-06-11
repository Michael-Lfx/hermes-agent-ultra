use axum::{extract::State, Json};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Deserialize;
use serde_json::json;

use hermes_tools::tools::tts::TtsBackend;

use crate::{
    error::{ok_json, AppError},
    state::AppState,
};

/// POST /api/audio/transcribe - Transcribe audio using OpenAI Whisper.
///
/// Request: `{ data_url: "data:audio/webm;base64,...", mime_type: "audio/webm" }`
/// Response: `{ ok: true, transcript: "...", provider: "openai" }`
#[derive(Debug, Deserialize)]
pub struct TranscribeRequest {
    pub data_url: String,
    pub mime_type: Option<String>,
}

pub async fn transcribe_audio(
    State(state): State<AppState>,
    Json(body): Json<TranscribeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Parse data URL: data:audio/webm;base64,...
    let data_url = body.data_url;
    let base64_data = data_url
        .split_once(',')
        .map(|(_, data)| data)
        .ok_or_else(|| AppError::BadRequest("Invalid data_url format".into()))?;

    let audio_bytes = B64
        .decode(base64_data)
        .map_err(|e| AppError::BadRequest(format!("Invalid base64: {}", e)))?;

    // Determine extension from mime_type or data_url
    let ext = body
        .mime_type
        .as_deref()
        .and_then(|m| m.split('/').nth(1))
        .map(|s| s.split(';').next().unwrap_or(s))
        .unwrap_or("webm");

    // Write to temp file
    let temp_path = std::env::temp_dir().join(format!("hermes_stt_{}.{}", uuid::Uuid::new_v4(), ext));
    tokio::fs::write(&temp_path, &audio_bytes)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write temp audio: {}", e)))?;

    // Transcribe using SttEngine
    let stt_config = hermes_config::voice::SttConfig::default();
    let engine = hermes_tools::voice_providers::SttEngine::new(stt_config);
    let transcript = engine
        .transcribe_file(temp_path.to_str().unwrap_or(""))
        .await
        .map_err(|e| AppError::Internal(format!("STT failed: {}", e)))?;

    // Clean up temp file (best effort)
    let _ = tokio::fs::remove_file(&temp_path).await;

    Ok(ok_json(json!({
        "ok": true,
        "transcript": transcript,
        "provider": "openai",
    })))
}

/// POST /api/audio/speak - Text-to-speech using OpenAI TTS.
///
/// Request: `{ text: "...", voice?: "alloy" }`
/// Response: `{ ok: true, data_url: "data:audio/mpeg;base64,...", mime_type: "audio/mpeg", provider: "openai" }`
#[derive(Debug, Deserialize)]
pub struct SpeakRequest {
    pub text: String,
    pub voice: Option<String>,
}

pub async fn speak_text(
    State(_state): State<AppState>,
    Json(body): Json<SpeakRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let voice = body.voice.as_deref().unwrap_or("alloy");

    // Use MultiTtsBackend to synthesize
    let backend = hermes_tools::backends::tts::MultiTtsBackend::new();
    let result_json = backend
        .synthesize(&body.text, Some(voice), Some("openai"))
        .await
        .map_err(|e| AppError::Internal(format!("TTS failed: {}", e)))?;

    // Parse result to get file path
    let result: serde_json::Value =
        serde_json::from_str(&result_json).map_err(|e| AppError::Internal(e.to_string()))?;

    let file_path = result
        .get("file")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("TTS result missing file path".into()))?;

    // Read audio file and encode as base64 data URL
    let audio_bytes = tokio::fs::read(file_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read audio file: {}", e)))?;

    // Clean up temp file (best effort)
    let _ = tokio::fs::remove_file(file_path).await;

    let data_url = format!("data:audio/mpeg;base64,{}", B64.encode(&audio_bytes));

    Ok(ok_json(json!({
        "ok": true,
        "data_url": data_url,
        "mime_type": "audio/mpeg",
        "provider": "openai",
    })))
}

/// GET /api/audio/elevenlabs/voices - List available voices (OpenAI voices).
///
/// Returns OpenAI voices in ElevenLabs-compatible format.
pub async fn list_voices() -> Result<Json<serde_json::Value>, AppError> {
    let voices = vec![
        json!({"voice_id": "alloy", "name": "Alloy", "label": "Alloy"}),
        json!({"voice_id": "echo", "name": "Echo", "label": "Echo"}),
        json!({"voice_id": "fable", "name": "Fable", "label": "Fable"}),
        json!({"voice_id": "onyx", "name": "Onyx", "label": "Onyx"}),
        json!({"voice_id": "nova", "name": "Nova", "label": "Nova"}),
        json!({"voice_id": "shimmer", "name": "Shimmer", "label": "Shimmer"}),
    ];

    Ok(ok_json(json!({
        "available": true,
        "voices": voices,
    })))
}
