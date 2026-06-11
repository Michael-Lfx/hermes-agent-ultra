use std::sync::atomic::{AtomicBool, Ordering};

/// Transport interface for JSON-RPC message delivery.
/// Implemented for stdio, WebSocket, and other channels.
pub trait Transport: Send + Sync {
    /// Write a JSON value to the transport.
    /// Returns false if the peer is disconnected.
    fn write(&self, obj: &serde_json::Value) -> bool;

    /// Close the transport (release resources).
    fn close(&self);
}

// ---------------------------------------------------------------------------
// WebSocket Transport
// ---------------------------------------------------------------------------

/// WebSocket transport using tokio mpsc channel.
pub struct WsTransport {
    tx: tokio::sync::mpsc::UnboundedSender<String>,
    closed: AtomicBool,
}

impl WsTransport {
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<String>) -> Self {
        Self {
            tx,
            closed: AtomicBool::new(false),
        }
    }

    /// Send a text frame via async channel.
    pub fn send_text(&self, text: String) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        match self.tx.send(text) {
            Ok(()) => true,
            Err(_) => {
                self.closed.store(true, Ordering::SeqCst);
                false
            }
        }
    }
}

impl Transport for WsTransport {
    fn write(&self, obj: &serde_json::Value) -> bool {
        match serde_json::to_string(obj) {
            Ok(json) => self.send_text(json),
            Err(_) => false,
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        // Channel will be dropped when all senders are gone
    }
}

// ---------------------------------------------------------------------------
// Null Transport (for disconnected sessions)
// ---------------------------------------------------------------------------

/// A transport that silently drops all messages.
/// Used when a WebSocket disconnects but the session is kept alive.
pub struct NullTransport;

impl Transport for NullTransport {
    fn write(&self, _obj: &serde_json::Value) -> bool {
        false
    }

    fn close(&self) {}
}

// ---------------------------------------------------------------------------
// Shared Transport Wrapper
// ---------------------------------------------------------------------------

/// Thread-safe wrapper around a Transport.
pub struct SharedTransport {
    inner: std::sync::Arc<dyn Transport>,
}

impl SharedTransport {
    pub fn new<T: Transport + 'static>(transport: T) -> Self {
        Self {
            inner: std::sync::Arc::new(transport),
        }
    }

    pub fn write(&self, obj: &serde_json::Value) -> bool {
        self.inner.write(obj)
    }

    pub fn close(&self) {
        self.inner.close();
    }
}

impl Clone for SharedTransport {
    fn clone(&self) -> Self {
        Self {
            inner: std::sync::Arc::clone(&self.inner),
        }
    }
}

// ---------------------------------------------------------------------------
// Replaceable Transport (for binding WS transport after session creation)
// ---------------------------------------------------------------------------

use std::sync::RwLock;

/// Transport that allows replacing the inner transport at runtime.
/// Used to bind a WebSocket transport to a session after `session.create`.
#[derive(Clone)]
pub struct ReplaceableTransport {
    inner: std::sync::Arc<RwLock<std::sync::Arc<dyn Transport>>>,
}

impl ReplaceableTransport {
    pub fn new<T: Transport + 'static>(initial: T) -> Self {
        Self {
            inner: std::sync::Arc::new(RwLock::new(std::sync::Arc::new(initial))),
        }
    }

    pub fn replace<T: Transport + 'static>(&self, transport: T) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = std::sync::Arc::new(transport);
        }
    }
}

impl Transport for ReplaceableTransport {
    fn write(&self, obj: &serde_json::Value) -> bool {
        self.inner
            .read()
            .map(|g| g.write(obj))
            .unwrap_or(false)
    }

    fn close(&self) {
        self.inner.read().map(|g| g.close()).unwrap_or(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_null_transport() {
        let t = NullTransport;
        assert!(!t.write(&json!({"test": true})));
        t.close();
    }

    #[test]
    fn test_ws_transport_send() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let t = WsTransport::new(tx);

        let msg = json!({"hello": "world"});
        assert!(t.write(&msg));

        let received = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&received).unwrap();
        assert_eq!(parsed["hello"], "world");
    }

    #[test]
    fn test_replaceable_transport() {
        let t = ReplaceableTransport::new(NullTransport);

        // Initially uses NullTransport (returns false)
        assert!(!t.write(&json!({"test": true})));

        // Replace with a WsTransport
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        t.replace(WsTransport::new(tx));

        let msg = json!({"replaced": true});
        assert!(t.write(&msg));

        let received = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&received).unwrap();
        assert_eq!(parsed["replaced"], true);
    }

    #[test]
    fn test_shared_transport_clone() {
        let t = SharedTransport::new(NullTransport);
        let cloned = t.clone();

        assert!(!t.write(&json!({"test": 1})));
        assert!(!cloned.write(&json!({"test": 2})));
    }
}
