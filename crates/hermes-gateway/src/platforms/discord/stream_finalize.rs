//! Discord streaming finalize: rate-limit backoff and duplicate-free fallback delivery.

use std::time::Duration;

use hermes_core::errors::GatewayError;
use hermes_core::traits::PlatformAdapter;
use tracing::warn;

/// Minimum interval between progressive Discord message edits during streaming.
pub const DISCORD_PROGRESSIVE_EDIT_MIN_MS: u64 = 1500;

const FINAL_EDIT_MAX_ATTEMPTS: u32 = 8;
const PLACEHOLDER_CLEAR_TEXT: &str = "…";

/// Parse `retry_after` from a Discord REST error embedded in [`GatewayError::SendFailed`].
pub fn discord_retry_after_secs(err: &GatewayError) -> Option<f64> {
    let GatewayError::SendFailed(msg) = err else {
        return None;
    };
    let json_start = msg.find('{')?;
    let value: serde_json::Value = serde_json::from_str(&msg[json_start..]).ok()?;
    value
        .get("retry_after")
        .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|n| n as f64)))
}

/// Edit a message, backing off when Discord returns HTTP 429.
pub async fn edit_message_with_retry(
    adapter: &dyn PlatformAdapter,
    chat_id: &str,
    message_id: &str,
    text: &str,
    max_attempts: u32,
) -> Result<(), GatewayError> {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match adapter.edit_message(chat_id, message_id, text).await {
            Ok(()) => return Ok(()),
            Err(err) if attempt < max_attempts => {
                if let Some(secs) = discord_retry_after_secs(&err) {
                    let wait = Duration::from_secs_f64(secs + 0.05);
                    tokio::time::sleep(wait).await;
                    continue;
                }
                return Err(err);
            }
            Err(err) => return Err(err),
        }
    }
}

/// Finalize a legacy streaming anchor: edit first chunk, then send overflow chunks.
///
/// On edit failure after retries, removes the placeholder when possible so a
/// fallback `send_message` does not duplicate partial progressive content.
pub async fn deliver_legacy_stream_final(
    adapter: &dyn PlatformAdapter,
    platform: &str,
    chat_id: &str,
    anchor_message_id: Option<&str>,
    chunks: &[String],
) -> Result<(), GatewayError> {
    if chunks.is_empty() {
        return Ok(());
    }

    let Some(message_id) = anchor_message_id.filter(|id| !id.is_empty()) else {
        for chunk in chunks {
            adapter.send_message(chat_id, chunk, None).await?;
        }
        return Ok(());
    };

    let first = &chunks[0];
    let edit_result = if platform == "discord" {
        edit_message_with_retry(adapter, chat_id, message_id, first, FINAL_EDIT_MAX_ATTEMPTS).await
    } else {
        adapter.edit_message(chat_id, message_id, first).await
    };

    match edit_result {
        Ok(()) => {
            for chunk in chunks.iter().skip(1) {
                adapter.send_message(chat_id, chunk, None).await?;
            }
            Ok(())
        }
        Err(err) => {
            warn!(
                platform = %platform,
                chat_id = %chat_id,
                message_id = %message_id,
                error = %err,
                "streaming final edit failed"
            );
            if platform == "discord" {
                deliver_discord_after_final_edit_failed(adapter, chat_id, message_id, chunks)
                    .await
            } else {
                adapter.send_message(chat_id, first, None).await?;
                for chunk in chunks.iter().skip(1) {
                    adapter.send_message(chat_id, chunk, None).await?;
                }
                Ok(())
            }
        }
    }
}

async fn deliver_discord_after_final_edit_failed(
    adapter: &dyn PlatformAdapter,
    chat_id: &str,
    message_id: &str,
    chunks: &[String],
) -> Result<(), GatewayError> {
    if adapter.delete_message(chat_id, message_id).await.is_ok() {
        for chunk in chunks {
            adapter.send_message(chat_id, chunk, None).await?;
        }
        return Ok(());
    }

    let _ = adapter
        .edit_message(chat_id, message_id, PLACEHOLDER_CLEAR_TEXT)
        .await;

    for chunk in chunks {
        adapter.send_message(chat_id, chunk, None).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_retry_after_from_discord_error() {
        let err = GatewayError::SendFailed(
            "Discord edit API error: {\"message\": \"You are being rate limited.\", \"retry_after\": 0.3, \"global\": false}".into(),
        );
        assert!((discord_retry_after_secs(&err).unwrap() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn retry_after_missing_for_non_send_failed() {
        let err = GatewayError::Platform("nope".into());
        assert!(discord_retry_after_secs(&err).is_none());
    }
}
