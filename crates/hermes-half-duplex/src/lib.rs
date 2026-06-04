pub mod asr;
pub mod audio;
pub mod bridge;
pub mod busy_replies;
pub mod config;
pub mod dashscope;
pub mod error;
pub mod kws;
pub mod orchestrator;
pub mod tts;
pub mod vad;
pub mod voice_session;

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::audio::AudioPlayback;
use crate::tts::TtsClient;

pub use bridge::{VoiceChatBridge, VoiceChatEvent, VoiceChatStatus};
pub use config::Config;
pub use error::{HalfDuplexError, Result};
pub use voice_session::VoiceChatSession;

/// Running voice chat: bridge for TUI, event stream, cancel handle.
pub struct VoiceChatHandle {
    pub bridge: Arc<VoiceChatBridge>,
    pub cancel: CancellationToken,
    task: Option<std::thread::JoinHandle<()>>,
}

impl VoiceChatHandle {
    pub async fn stop(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

/// Start mic/ASR/TTS loop; returns handle plus events for the UI layer.
pub async fn spawn_voice_chat(
    cfg: Config,
) -> Result<(VoiceChatHandle, mpsc::Receiver<VoiceChatEvent>)> {
    let (event_tx, event_rx) = mpsc::channel(64);
    let (tts, tts_rx) = TtsClient::connect(&cfg.dashscope, &cfg.tts).await?;
    let playback = Arc::new(AudioPlayback::start(
        &cfg.audio,
        cfg.tts.sample_rate,
    )?);
    let play_gen = Arc::new(AtomicU64::new(0));
    let bridge = VoiceChatBridge::new(
        tts,
        cfg.orchestrator.clone(),
        event_tx,
        cfg.busy_replies.cooldown_secs,
        playback.clone(),
        play_gen.clone(),
    );
    let cancel = CancellationToken::new();
    let session = VoiceChatSession::new(
        cfg,
        bridge.clone(),
        cancel.clone(),
        tts_rx,
        playback,
        play_gen,
    );
    // WebRtcVad is not `Send`; run the capture/ASR loop on a dedicated thread.
    let bridge_err = bridge.clone();
    let task = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("voice chat runtime");
        rt.block_on(async move {
            if let Err(e) = session.run().await {
                tracing::error!(error = %e, "voice chat session ended with error");
                bridge_err.send_error(e.to_string()).await;
            }
        });
    });
    Ok((
        VoiceChatHandle {
            bridge,
            cancel,
            task: Some(task),
        },
        event_rx,
    ))
}
