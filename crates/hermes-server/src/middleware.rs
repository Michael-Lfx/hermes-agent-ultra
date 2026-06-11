use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::state::AppState;

/// Public API paths that do not require authentication.
const PUBLIC_PATHS: &[&str] = &[
    "/api/status",
    "/health",
    "/api/auth/providers",
    "/api/auth/ws-ticket",
    "/api/ws",  // WebSocket has its own auth (token/ticket query params)
    "/login",
    "/auth/callback",
];

/// Authentication middleware for all /api/* routes.
/// Validates X-Hermes-Session-Token header or Authorization: Bearer token.
/// Public paths are allowed without authentication.
pub async fn request_guard(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    
    // Allow public paths without authentication
    if PUBLIC_PATHS.iter().any(|p| path.starts_with(p)) {
        return next.run(request).await;
    }
    
    // Extract token from X-Hermes-Session-Token header
    let token = request
        .headers()
        .get("X-Hermes-Session-Token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| extract_bearer_token(request.headers()));
    
    match token {
        Some(t) if constant_time_eq(&t, state.session_token()) => {
            next.run(request).await
        }
        _ => {
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"detail":"Unauthorized"}"#))
                .unwrap()
        }
    }
}

/// Extract Bearer token from Authorization header.
fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    
    if a.len() != b.len() {
        // Still perform constant-time comparison on padded buffers
        // to avoid leaking length information
        let max_len = a.len().max(b.len());
        let mut a_buf = vec![0u8; max_len];
        let mut b_buf = vec![0u8; max_len];
        a_buf[..a.len().min(max_len)].copy_from_slice(&a.as_bytes()[..a.len().min(max_len)]);
        b_buf[..b.len().min(max_len)].copy_from_slice(&b.as_bytes()[..b.len().min(max_len)]);
        a_buf.ct_eq(&b_buf).into()
    } else {
        a.as_bytes().ct_eq(b.as_bytes()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("test", "test"));
        assert!(!constant_time_eq("test", "tent"));
        assert!(!constant_time_eq("test", "testing"));
    }

    #[test]
    fn test_extract_bearer_token() {
        let mut headers = axum::http::HeaderMap::new();
        assert_eq!(extract_bearer_token(&headers), None);
        
        headers.insert("Authorization", "Bearer abc123".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers), Some("abc123".to_string()));
        
        headers.insert("Authorization", "Basic xyz".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers), None);
    }
}
