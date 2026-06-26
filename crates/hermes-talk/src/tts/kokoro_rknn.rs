//! In-process Kokoro RKNN TTS via libkokoro FFI (RK3588 NPU decoder).

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

use crate::config::KokoroRknnTtsConfig;
use crate::error::{DemoError, Result};
use crate::tts::kokoro_ffi::KokoroEngineHandle;
use crate::tts::{TtsEngine, bailian::TtsAudio};

enum TtsCommand {
    AppendText {
        text: String,
        done: oneshot::Sender<Result<()>>,
    },
    FinishTurn(oneshot::Sender<Result<()>>),
    InterruptTurn(oneshot::Sender<Result<()>>),
}

pub struct KokoroRknnTts {
    cmd_tx: mpsc::Sender<TtsCommand>,
}

impl KokoroRknnTts {
    pub async fn connect(cfg: &KokoroRknnTtsConfig) -> Result<(Self, mpsc::Receiver<TtsAudio>)> {
        let (audio_tx, audio_rx) = mpsc::channel(128);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TtsCommand>(32);
        let cfg = cfg.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_driver(cfg, cmd_rx, audio_tx) {
                tracing::error!(error = %e, "kokoro RKNN tts driver exited");
            }
        });

        Ok((Self { cmd_tx }, audio_rx))
    }
}

#[async_trait]
impl TtsEngine for KokoroRknnTts {
    async fn warmup(&self) -> Result<()> {
        Ok(())
    }

    async fn append_text(&self, text: &str) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TtsCommand::AppendText {
                text: text.to_string(),
                done: tx,
            })
            .await
            .map_err(|e| DemoError::Tts(e.to_string()))?;
        rx.await.map_err(|e| DemoError::Tts(e.to_string()))?
    }

    async fn finish_turn(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TtsCommand::FinishTurn(tx))
            .await
            .map_err(|e| DemoError::Tts(e.to_string()))?;
        match tokio::time::timeout(std::time::Duration::from_secs(120), rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => Err(DemoError::Tts(e.to_string())),
            Err(_) => Err(DemoError::Tts("kokoro RKNN finish-turn timeout".into())),
        }
    }

    async fn interrupt_turn(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TtsCommand::InterruptTurn(tx))
            .await
            .map_err(|e| DemoError::Tts(e.to_string()))?;
        rx.await.map_err(|e| DemoError::Tts(e.to_string()))?
    }
}

fn run_driver(
    cfg: KokoroRknnTtsConfig,
    mut cmd_rx: mpsc::Receiver<TtsCommand>,
    audio_tx: mpsc::Sender<TtsAudio>,
) -> Result<()> {
    let engine = KokoroEngineHandle::create(&cfg)?;
    info!(
        engine = "kokoro_hybrid_v1",
        model_dir = %cfg.model_dir,
        front = %cfg.front_rknn,
        voice = %cfg.voice,
        seq_len = cfg.seq_len,
        "Kokoro hybrid-v1 RKNN TTS ready"
    );

    let mut text_buf = String::new();
    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            TtsCommand::AppendText { text, done } => {
                text_buf.push_str(&text);
                let _ = done.send(Ok(()));
            }
            TtsCommand::FinishTurn(done) => {
                if text_buf.is_empty() {
                    let _ = done.send(Ok(()));
                    continue;
                }
                let text = std::mem::take(&mut text_buf);
                let result = synthesize_turn(&engine, &cfg, &text, &audio_tx);
                let _ = done.send(result);
            }
            TtsCommand::InterruptTurn(done) => {
                text_buf.clear();
                let _ = done.send(Ok(()));
            }
        }
    }
    Ok(())
}

fn synthesize_turn(
    engine: &KokoroEngineHandle,
    cfg: &KokoroRknnTtsConfig,
    text: &str,
    audio_tx: &mpsc::Sender<TtsAudio>,
) -> Result<()> {
    engine.synthesize_text(text, &cfg.voice, cfg.speed, |chunk| {
        let pcm = i16_to_le_bytes(chunk);
        if !pcm.is_empty() {
            let _ = audio_tx.blocking_send(TtsAudio { pcm });
        }
    })
}

fn i16_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}
