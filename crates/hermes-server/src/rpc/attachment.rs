use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde_json::json;

use crate::{
    state::AppState,
    ws::rpc::{JsonRpcRequest, JsonRpcResponse},
};

/// image.attach - Attach a local image file to a session.
///
/// Params: `{ session_id, path }`
/// Returns: `{ ok: true, url: "file://..." }`
pub async fn handle_image_attach(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    let path = params.get("path")?.as_str()?;

    let upload_dir = state.hermes_home.join("uploads").join(session_id);
    tokio::fs::create_dir_all(&upload_dir).await.ok()?;

    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let file_name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let dest_path = upload_dir.join(&file_name);

    match tokio::fs::copy(path, &dest_path).await {
        Ok(_) => {
            let url = format!("file://{}", dest_path.to_string_lossy());
            Some(JsonRpcResponse::ok(request.id, json!({ "ok": true, "url": url })))
        }
        Err(e) => Some(JsonRpcResponse::err(
            request.id,
            crate::ws::rpc::JsonRpcError::server_error(5001, format!("Failed to copy image: {}", e)),
        )),
    }
}

/// image.attach_bytes - Attach an image from base64 data to a session.
///
/// Params: `{ session_id, content_base64, filename }`
/// Returns: `{ ok: true, url: "data:image/..." }`
pub async fn handle_image_attach_bytes(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    let content_base64 = params.get("content_base64")?.as_str()?;
    let filename = params.get("filename")?.as_str()?;

    let bytes = match B64.decode(content_base64) {
        Ok(b) => b,
        Err(e) => {
            return Some(JsonRpcResponse::err(
                request.id,
                crate::ws::rpc::JsonRpcError::server_error(4001, format!("Invalid base64: {}", e)),
            ));
        }
    };

    let upload_dir = state.hermes_home.join("uploads").join(session_id);
    tokio::fs::create_dir_all(&upload_dir).await.ok()?;

    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let file_name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let file_path = upload_dir.join(&file_name);

    if let Err(e) = tokio::fs::write(&file_path, &bytes).await {
        return Some(JsonRpcResponse::err(
            request.id,
            crate::ws::rpc::JsonRpcError::server_error(5001, format!("Failed to write image: {}", e)),
        ));
    }

    let mime = match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    };
    let data_url = format!("data:{};base64,{}", mime, B64.encode(&bytes));

    Some(JsonRpcResponse::ok(request.id, json!({
        "ok": true,
        "url": data_url,
        "file_path": file_path.to_string_lossy(),
    })))
}

/// file.attach - Attach a file (image or non-image) to a session.
///
/// Params (local mode): `{ session_id, name, path }`
/// Params (remote mode): `{ session_id, name, data_url }`
/// Returns: `{ ok: true, ref: "@file:filename" }`
pub async fn handle_file_attach(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    let params = request.params.as_ref()?.as_object()?;
    let session_id = params.get("session_id")?.as_str()?;
    let name = params.get("name")?.as_str()?;

    let upload_dir = state.hermes_home.join("uploads").join(session_id);
    tokio::fs::create_dir_all(&upload_dir).await.ok()?;

    let file_ref = format!("{}-{}", &uuid::Uuid::new_v4().to_string()[..8], name);
    let file_path = upload_dir.join(&file_ref);

    // Local mode: copy from path
    if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
        if let Err(e) = tokio::fs::copy(path, &file_path).await {
            return Some(JsonRpcResponse::err(
                request.id,
                crate::ws::rpc::JsonRpcError::server_error(5001, format!("Failed to copy file: {}", e)),
            ));
        }
    }
    // Remote mode: decode from data_url
    else if let Some(data_url) = params.get("data_url").and_then(|v| v.as_str()) {
        let base64_data = match data_url.split_once(',').map(|(_, d)| d) {
            Some(d) => d,
            None => {
                return Some(JsonRpcResponse::err(
                    request.id,
                    crate::ws::rpc::JsonRpcError::server_error(4001, "Invalid data_url".into()),
                ));
            }
        };
        let bytes = match B64.decode(base64_data) {
            Ok(b) => b,
            Err(e) => {
                return Some(JsonRpcResponse::err(
                    request.id,
                    crate::ws::rpc::JsonRpcError::server_error(4001, format!("Invalid base64: {}", e)),
                ));
            }
        };
        if let Err(e) = tokio::fs::write(&file_path, &bytes).await {
            return Some(JsonRpcResponse::err(
                request.id,
                crate::ws::rpc::JsonRpcError::server_error(5001, format!("Failed to write file: {}", e)),
            ));
        }
    } else {
        return Some(JsonRpcResponse::err(
            request.id,
            crate::ws::rpc::JsonRpcError::server_error(4001, "Missing path or data_url".into()),
        ));
    }

    Some(JsonRpcResponse::ok(request.id, json!({
        "ok": true,
        "ref": format!("@file:{}", name),
        "file_path": file_path.to_string_lossy(),
    })))
}
