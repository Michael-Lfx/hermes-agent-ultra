//! In-process Kokoro RKNN TTS via libkokoro FFI (RK3588 NPU decoder).
//!
//! Hybrid RKNN `tokens.txt` uses Bopomofo/phoneme symbols (see rkvoice-stream). Han text
//! is routed to sherpa CPU Kokoro (`kokoro-multi-lang-v1_1`) which includes zh/en G2P.

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::config::{KokoroRknnTtsConfig, SherpaTtsRuntime};
use crate::error::{DemoError, Result};
use crate::tts::kokoro_ffi::KokoroEngineHandle;
use crate::tts::sherpa_tts::SherpaKokoroEngine;
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
    pub async fn connect(
        rk_cfg: &KokoroRknnTtsConfig,
        sherpa_fallback: &SherpaTtsRuntime,
    ) -> Result<(Self, mpsc::Receiver<TtsAudio>)> {
        #[cfg(not(kokoro_rknn_ffi))]
        {
            let _ = (rk_cfg, sherpa_fallback);
            return Err(DemoError::Tts(
                "Kokoro hybrid RKNN FFI not linked (run make prefetch-talk-aarch64 && rebuild with libkokoro_ffi.a)"
                    .into(),
            ));
        }
        let (audio_tx, audio_rx) = mpsc::channel(128);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TtsCommand>(32);
        let rk_cfg = rk_cfg.clone();
        let sherpa_fallback = sherpa_fallback.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_driver(rk_cfg, sherpa_fallback, cmd_rx, audio_tx) {
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
    rk_cfg: KokoroRknnTtsConfig,
    sherpa_fallback: SherpaTtsRuntime,
    mut cmd_rx: mpsc::Receiver<TtsCommand>,
    audio_tx: mpsc::Sender<TtsAudio>,
) -> Result<()> {
    let engine = KokoroEngineHandle::create(&rk_cfg)?;
    let sherpa = SherpaKokoroEngine::open(&sherpa_fallback)?;
    info!(
        engine = "kokoro_hybrid_v1",
        model_dir = %rk_cfg.model_dir,
        front = %rk_cfg.front_rknn,
        voice = %rk_cfg.voice,
        seq_len = rk_cfg.seq_len,
        sherpa_model = %sherpa_fallback.kokoro.model,
        "Kokoro hybrid-v1 RKNN TTS ready (Han -> sherpa CPU, ASCII -> RKNN)"
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
                let result = synthesize_turn(&engine, &sherpa, &rk_cfg, &text, &audio_tx);
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

/// Hybrid RKNN tokens are phoneme/Bopomofo; CJK and fullwidth text needs sherpa G2P.
fn needs_sherpa_g2p(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            '\u{4E00}'..='\u{9FFF}'
                | '\u{3400}'..='\u{4DBF}'
                | '\u{F900}'..='\u{FAFF}'
                | '\u{3000}'..='\u{303F}'
                | '\u{FF00}'..='\u{FFEF}'
        )
    })
}

fn synthesize_turn(
    engine: &KokoroEngineHandle,
    sherpa: &SherpaKokoroEngine,
    cfg: &KokoroRknnTtsConfig,
    text: &str,
    audio_tx: &mpsc::Sender<TtsAudio>,
) -> Result<()> {
    if needs_sherpa_g2p(text) {
        info!(chars = text.chars().count(), "tts route: sherpa CPU (zh/CJK text)");
        return sherpa.synthesize_turn(text, audio_tx);
    }

    info!(chars = text.chars().count(), "tts route: kokoro hybrid RKNN");
    match synthesize_rknn_turn(engine, cfg, text, audio_tx) {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("zero tokens") || msg.contains("zero phoneme") {
                warn!(error = %msg, "kokoro RKNN tokenize failed; falling back to sherpa CPU");
                sherpa.synthesize_turn(text, audio_tx)
            } else {
                Err(e)
            }
        }
    }
}

fn synthesize_rknn_turn(
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
