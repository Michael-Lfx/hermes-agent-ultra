//! Toolset system
//!
//! Manages named groups of tools (toolsets) with:
//! - Predefined toolset definitions for all built-in tool groups
//! - Recursive resolution with cycle detection
//! - Custom toolset creation at runtime
//! - Integration with ToolRegistry for plugin-registered toolsets

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::registry::ToolRegistry;

// ---------------------------------------------------------------------------
// Predefined toolset constants
// ---------------------------------------------------------------------------

/// Web search and extraction tools.
pub const TOOLSET_WEB: &[&str] = &["web_search", "web_extract"];
/// Reusable content retrieval framework tools.
pub const TOOLSET_CONTENT: &[&str] = &["content_plan", "content_execute", "content_normalize"];
/// Terminal command execution tools.
pub const TOOLSET_TERMINAL: &[&str] = &["terminal", "process", "process_registry"];
/// File system tools.
pub const TOOLSET_FILE: &[&str] = &["read_file", "write_file", "patch", "search_files"];
/// Browser automation tools.
pub const TOOLSET_BROWSER: &[&str] = &[
    "browser_navigate",
    "browser_snapshot",
    "browser_click",
    "browser_type",
    "browser_scroll",
    "browser_back",
    "browser_press",
    "browser_get_images",
    "browser_vision",
    "browser_console",
];
/// Vision analysis tools.
pub const TOOLSET_VISION: &[&str] = &["vision_analyze", "video_analyze"];
/// Image generation tools.
pub const TOOLSET_IMAGE_GEN: &[&str] = &["image_generate"];
/// Video generation tools.
pub const TOOLSET_VIDEO_GEN: &[&str] = &["video_generate"];
/// Spotify Web API tools.
pub const TOOLSET_SPOTIFY: &[&str] = &[
    "spotify_playback",
    "spotify_devices",
    "spotify_queue",
    "spotify_search",
    "spotify_playlists",
    "spotify_albums",
    "spotify_library",
];
/// Skills management tools.
pub const TOOLSET_SKILLS: &[&str] = &["skills_list", "skill_view", "skill_manage"];
/// Persistent memory tools.
pub const TOOLSET_MEMORY: &[&str] = &["memory"];
/// Session search tools.
pub const TOOLSET_SESSION_SEARCH: &[&str] = &["session_search"];
/// Todo/task management tools.
pub const TOOLSET_TODO: &[&str] = &["todo"];
/// Clarification/question tools.
pub const TOOLSET_CLARIFY: &[&str] = &["clarify"];
/// Code execution tools.
pub const TOOLSET_CODE_EXECUTION: &[&str] = &["execute_code"];
/// Task delegation tools.
pub const TOOLSET_DELEGATION: &[&str] = &["delegate_task"];
/// Cron job management tools.
pub const TOOLSET_CRONJOB: &[&str] = &["cronjob"];
/// Cross-platform messaging tools.
pub const TOOLSET_MESSAGING: &[&str] = &["send_message"];
/// Home Assistant integration tools.
pub const TOOLSET_HOMEASSISTANT: &[&str] = &[
    "ha_list_entities",
    "ha_get_state",
    "ha_list_services",
    "ha_call_service",
];
/// Text-to-speech tools.
pub const TOOLSET_TTS: &[&str] = &["text_to_speech", "tts_premium"];
/// Voice input/mode tools.
pub const TOOLSET_VOICE: &[&str] = &["transcription", "voice_mode"];
/// Security helpers.
pub const TOOLSET_SECURITY: &[&str] = &["osv_check", "url_safety"];
/// System utility helpers.
pub const TOOLSET_SYSTEM: &[&str] = &["env_passthrough", "credential_files", "tool_result_storage"];
/// Mixture-of-agents workflow.
pub const TOOLSET_MIXTURE_OF_AGENTS: &[&str] = &["mixture_of_agents"];
/// Background desktop automation (macOS).
pub const TOOLSET_COMPUTER_USE: &[&str] = &["computer_use"];
/// Feishu/Lark OpenAPI tools (calendar, docs, tasks, chat history).
pub const TOOLSET_FEISHU: &[&str] = &[
    "feishu_calendar",
    "feishu_docs",
    "feishu_task",
    "feishu_chat_history",
];
/// Quick capture inbox (fragments + optional reminders).
pub const TOOLSET_CAPTURE: &[&str] = &["capture"];
/// Live spot quote (no backtest / OHLCV).
pub const TOOLSET_TRADING_QUOTE: &[&str] = &["get_quote"];
/// Full quantitative research tools (OHLCV, backtest, strategies).
pub const TOOLSET_TRADING: &[&str] = &[
    "get_quote",
    "resolve_a_share_symbol",
    "get_market_data",
    "run_backtest",
    "get_backtest_report",
    "list_strategies",
    "create_strategy",
    "analyze_stock",
];

        // Platform composite toolsets
        self.register(Toolset::with_includes(
            "hermes-cli",
            vec![
                "web",
                "content",
                "capture",
                "terminal",
                "file",
                "browser",
                "vision",
                "image_gen",
                "video_gen",
                "spotify",
                "skills",
                "memory",
                "session_search",
                "todo",
                "clarify",
                "code_execution",
                "delegation",
                "cronjob",
                "messaging",
                "homeassistant",
                "tts",
                "computer_use",
                "trading-quote",
                "trading",
    }

    #[test]
    fn test_messaging_platform_presets_present() {
        let manager = ToolsetManager::new(empty_registry());
        for preset in [
            "hermes-telegram",
            "hermes-discord",
            "hermes-whatsapp",
            "hermes-slack",
        ] {
            let tools = manager.resolve_toolset_unfiltered(preset).unwrap();
            assert!(
                tools.contains(&"send_message".to_string()),
                "preset {preset} should include send_message"
            );
            assert!(
                tools.contains(&"terminal".to_string()),
                "preset {preset} should include terminal"
            );
            assert!(
                tools.contains(&"image_generate".to_string()),
                "preset {preset} should include image_generate"
            );
            assert!(
                tools.contains(&"cronjob".to_string()),
                "preset {preset} should include cronjob"
            );
            assert!(
                tools.contains(&"session_search".to_string()),
                "preset {preset} should include session_search"
            );
            assert!(
                tools.contains(&"text_to_speech".to_string()),
                "preset {preset} should include text_to_speech"
            );
            assert!(
                tools.contains(&"ha_call_service".to_string()),
                "preset {preset} should include homeassistant tools"
            );
        }
    }
}
