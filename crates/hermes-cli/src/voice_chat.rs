//! Pure voice chat mode (`/chat`) using DashScope streaming ASR/TTS.

use std::path::PathBuf;
use std::sync::Arc;

use hermes_config::hermes_home;
use hermes_half_duplex::{
    spawn_voice_chat, Config, VoiceChatBridge, VoiceChatEvent, VoiceChatHandle, VoiceChatStatus,
};
use tokio::sync::mpsc;

use crate::tui::Event;

pub fn half_duplex_config_path() -> PathBuf {
    hermes_home().join("half_duplex.toml")
}

pub async fn start_voice_chat(
    event_tx: mpsc::UnboundedSender<Event>,
) -> Result<VoiceChatSession, String> {
    let path = half_duplex_config_path();
    if !path.is_file() {
        return Err(format!(
            "missing {}; copy crates/hermes-half-duplex/half_duplex.example.toml to {}",
            path.display(),
            path.display()
        ));
    }
    let cfg = Config::load(&path).map_err(|e| e.to_string())?;
    let wake_enabled = cfg.wake.enabled;
    let (handle, mut events) = spawn_voice_chat(cfg).await.map_err(|e| e.to_string())?;
    let bridge = handle.bridge.clone();
    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            let mapped = match ev {
                VoiceChatEvent::UserUtterance(text) => Event::VoiceChatUserText(text),
                VoiceChatEvent::InterruptForUtterance(text) => {
                    Event::VoiceChatInterruptForUtterance(text)
                }
                VoiceChatEvent::BusyReply { phrase, heard } => {
                    Event::VoiceChatBusyReply { phrase, heard }
                }
                VoiceChatEvent::TurnComplete => Event::VoiceChatTurnComplete,
                VoiceChatEvent::WakeAccepted => Event::VoiceChatWakeAccepted,
                VoiceChatEvent::BargeIn => Event::VoiceChatBargeIn,
                VoiceChatEvent::Status(st) => Event::VoiceChatStatus(status_label(st)),
                VoiceChatEvent::Error(msg) => Event::VoiceChatError(msg),
            };
            if event_tx.send(mapped).is_err() {
                break;
            }
        }
    });
    Ok(VoiceChatSession {
        handle,
        bridge,
        wake_enabled,
    })
}

pub struct VoiceChatSession {
    handle: VoiceChatHandle,
    pub bridge: Arc<VoiceChatBridge>,
    pub wake_enabled: bool,
}

impl VoiceChatSession {
    pub async fn stop(self) {
        self.handle.stop().await;
    }
}

fn status_label(st: VoiceChatStatus) -> String {
    match st {
        VoiceChatStatus::WaitingWake => "Waiting wake word".to_string(),
        VoiceChatStatus::Listening => "Listening".to_string(),
        VoiceChatStatus::Thinking => "Thinking".to_string(),
        VoiceChatStatus::Speaking => "Speaking".to_string(),
    }
}
