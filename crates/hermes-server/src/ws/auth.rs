//! WebSocket authentication helpers.

use subtle::ConstantTimeEq;

use crate::state::AppState;

/// Authenticate a WebSocket connection.
///
/// Supports token-based auth via `?token=` query parameter.
/// Returns true if authentication succeeds.
pub fn authenticate_ws(
    query_params: &[(String, String)],
    state: &AppState,
) -> bool {
    // Look for ?token=... or ?ticket=... in query params
    let token = query_params
        .iter()
        .find(|(k, _)| k == "token" || k == "ticket")
        .map(|(_, v)| v.as_str());

    match token {
        Some(t) => {
            let expected = state.session_token();
            // Constant-time comparison to prevent timing attacks
            t.as_bytes().ct_eq(expected.as_bytes()).into()
        }
        None => {
            tracing::warn!("WS connection rejected: missing token");
            false
        }
    }
}

/// Parse query string into key-value pairs.
pub fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.to_string();
            let value = parts.next().unwrap_or("").to_string();
            Some((key, value))
        })
        .collect()
}
