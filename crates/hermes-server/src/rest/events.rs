use axum::{
    extract::State,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use serde_json::json;
use std::convert::Infallible;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::state::AppState;

/// POST /api/pub - Publish an event to all SSE subscribers.
pub async fn publish_event(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    state.publish_event(payload);
    Json(json!({"status": "ok"}))
}

/// GET /api/events - Subscribe to server-sent events.
pub async fn subscribe_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.subscribe_events();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(value) => Some(Ok(Event::default().data(value.to_string()))),
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => None,
    });
    Sse::new(stream)
}
