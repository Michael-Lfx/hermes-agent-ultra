//! Event type definitions for JSON-RPC server push.

/// All event types that can be sent from server to client.
pub mod types {
    pub const GATEWAY_READY: &str = "gateway.ready";
    pub const SESSION_INFO: &str = "session.info";
    pub const THINKING_DELTA: &str = "thinking.delta";
    pub const REASONING_DELTA: &str = "reasoning.delta";
    pub const REASONING_AVAILABLE: &str = "reasoning.available";
    pub const TOOL_START: &str = "tool.start";
    pub const TOOL_COMPLETE: &str = "tool.complete";
    pub const TOOL_GENERATING: &str = "tool.generating";
    pub const TOOL_PROGRESS: &str = "tool.progress";
    pub const STATUS_UPDATE: &str = "status.update";
    pub const MESSAGE_START: &str = "message.start";
    pub const MESSAGE_DELTA: &str = "message.delta";
    pub const MESSAGE_COMPLETE: &str = "message.complete";
    pub const APPROVAL_REQUEST: &str = "approval.request";
    pub const CLARIFY_REQUEST: &str = "clarify.request";
    pub const SUDO_REQUEST: &str = "sudo.request";
    pub const SECRET_REQUEST: &str = "secret.request";
    pub const NOTIFICATION_SHOW: &str = "notification.show";
    pub const NOTIFICATION_CLEAR: &str = "notification.clear";
    pub const BACKGROUND_COMPLETE: &str = "background.complete";
    pub const PREVIEW_RESTART_PROGRESS: &str = "preview.restart.progress";
    pub const PREVIEW_RESTART_COMPLETE: &str = "preview.restart.complete";
    pub const SKIN_CHANGED: &str = "skin.changed";
    pub const ERROR: &str = "error";
}
