//! Kokoro TTS via [kokoro-server](https://github.com/...) HTTP API (RK3588 NPU decoder).

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use crate::config::KokoroServerTtsConfig;
use crate::error::{DemoError, Result};
use crate::tts::{TtsEngine, bailian::TtsAudio};

enum TtsCommand {
    AppendText {
        text: String,
        done: oneshot::Sender<Result<()>>,
    },
    FinishTurn(oneshot::Sender<Result<()>>),
    InterruptTurn(oneshot::Sender<Result<()>>),
}

#[derive(Serialize)]
struct SynthRequest<'a> {
    text: &'a str,
    voice: &'a str,
    speed: f32,
    british: bool,
    audio_format: &'a str,
}

pub struct KokoroServerTts {
    cmd_tx: mpsc::Sender<TtsCommand>,
}

impl KokoroServerTts {
    pub async fn connect(cfg: &KokoroServerTtsConfig) -> Result<(Self, mpsc::Receiver<TtsAudio>)> {
        let (audio_tx, audio_rx) = mpsc::channel(128);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TtsCommand>(32);
        let cfg = cfg.clone();

        tokio::spawn(async move {
            if let Err(e) = run_driver(cfg, cmd_rx, audio_tx).await {
                error!(error = %e, "kokoro-server tts driver exited");
            }
        });

        Ok((Self { cmd_tx }, audio_rx))
    }
}

#[async_trait]
impl TtsEngine for KokoroServerTts {
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
            Err(_) => Err(DemoError::Tts("kokoro-server finish-turn timeout".into())),
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

async fn run_driver(
    cfg: KokoroServerTtsConfig,
    mut cmd_rx: mpsc::Receiver<TtsCommand>,
    audio_tx: mpsc::Sender<TtsAudio>,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| DemoError::Tts(format!("kokoro-server http client: {e}")))?;

    let synth_url = format!(
        "{}/api/v1/synthesise",
        cfg.base_url.trim().trim_end_matches('/')
    );

    info!(
        url = %synth_url,
        voice = %cfg.voice,
        "kokoro-server TTS ready"
    );

    let mut text_buf = String::new();

    while let Some(cmd) = cmd_rx.recv().await {
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
                let result = synthesize(&client, &synth_url, &cfg, &text, &audio_tx).await;
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

async fn synthesize(
    client: &reqwest::Client,
    url: &str,
    cfg: &KokoroServerTtsConfig,
    text: &str,
    audio_tx: &mpsc::Sender<TtsAudio>,
) -> Result<()> {
    let body = SynthRequest {
        text,
        voice: &cfg.voice,
        speed: cfg.speed,
        british: cfg.british,
        audio_format: &cfg.audio_format,
    };

    let mut req = client.post(url).json(&body);
    if !cfg.auth_token.is_empty() {
        req = req.bearer_auth(&cfg.auth_token);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| DemoError::Tts(format!("kokoro-server request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        return Err(DemoError::Tts(format!(
            "kokoro-server HTTP {status}: {err_body}"
        )));
    }

    let pcm = resp
        .bytes()
        .await
        .map_err(|e| DemoError::Tts(format!("kokoro-server read body: {e}")))?;

    if pcm.is_empty() {
        warn!("kokoro-server returned empty audio");
        return Ok(());
    }

    audio_tx
        .send(TtsAudio { pcm: pcm.to_vec() })
        .await
        .map_err(|e| DemoError::Tts(format!("kokoro-server audio channel: {e}")))?;

    Ok(())
}
