use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, warn};

use crate::config::{DashscopeConfig, TtsConfig};
use crate::dashscope::{self, continue_task, event_name, finish_task, run_task_tts};
use crate::error::{HalfDuplexError, Result};

pub struct TtsAudio {
    pub pcm: Vec<u8>,
}

enum TtsCommand {
    AppendText {
        text: String,
        done: oneshot::Sender<Result<()>>,
    },
    FinishTurn(oneshot::Sender<Result<()>>),
    WarmupStart(oneshot::Sender<Result<()>>),
    /// finish-task and wait for task-finished; do not start a new task (next append will).
    InterruptTurn(oneshot::Sender<Result<()>>),
}

#[derive(Clone)]
pub struct TtsClient {
    cmd_tx: mpsc::Sender<TtsCommand>,
    finish_timeout_sec: u64,
}

impl TtsClient {
    pub async fn connect(
        dashscope: &DashscopeConfig,
        tts: &TtsConfig,
    ) -> Result<(Self, mpsc::Receiver<TtsAudio>)> {
        let (audio_tx, audio_rx) = mpsc::channel(128);
        let (cmd_tx, cmd_rx) = mpsc::channel(32);

        let url = dashscope.ws_url.clone();
        let api_key = dashscope.api_key.clone();
        let model = tts.model.clone();
        let voice = tts.voice.clone();
        let sample_rate = tts.sample_rate;
        let format = tts.format.clone();
        let task_started_timeout_sec = tts.task_started_timeout_sec;
        let finish_timeout_sec = tts.finish_timeout_sec;

        tokio::spawn(async move {
            if let Err(e) = run_tts_driver(
                &url,
                &api_key,
                &model,
                &voice,
                sample_rate,
                &format,
                task_started_timeout_sec,
                cmd_rx,
                audio_tx,
            )
            .await
            {
                error!(error = %e, "tts driver exited");
            }
        });

        Ok((
            Self {
                cmd_tx,
                finish_timeout_sec,
            },
            audio_rx,
        ))
    }

    pub async fn warmup(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TtsCommand::WarmupStart(tx))
            .await
            .map_err(|e| HalfDuplexError::Tts(e.to_string()))?;
        rx.await
            .map_err(|e| HalfDuplexError::Tts(e.to_string()))?
    }

    pub async fn append_text(&self, text: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TtsCommand::AppendText { text, done: tx })
            .await
            .map_err(|e| HalfDuplexError::Tts(e.to_string()))?;
        rx.await
            .map_err(|e| HalfDuplexError::Tts(e.to_string()))?
    }

    pub fn finish_turn(&self) -> oneshot::Receiver<Result<()>> {
        let (tx, rx) = oneshot::channel();
        let _ = self.cmd_tx.try_send(TtsCommand::FinishTurn(tx));
        rx
    }

    pub async fn finish_turn_async_with_timeout(&self, timeout_secs: u64) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TtsCommand::FinishTurn(tx))
            .await
            .map_err(|e| HalfDuplexError::Tts(e.to_string()))?;
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => Err(HalfDuplexError::Tts(e.to_string())),
            Err(_) => Err(HalfDuplexError::Tts("finish-task timeout".into())),
        }
    }

    pub async fn finish_turn_async(&self) -> Result<()> {
        self.finish_turn_async_with_timeout(self.finish_timeout_sec)
            .await
    }

    /// Stop the current TTS task (barge-in). Waits for task-finished before returning.
    pub async fn interrupt_turn(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TtsCommand::InterruptTurn(tx))
            .await
            .map_err(|e| HalfDuplexError::Tts(e.to_string()))?;
        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => Err(HalfDuplexError::Tts(e.to_string())),
            Err(_) => Err(HalfDuplexError::Tts("interrupt-turn timeout".into())),
        }
    }
}

async fn run_tts_driver(
    url: &str,
    api_key: &str,
    model: &str,
    voice: &str,
    sample_rate: u32,
    format: &str,
    task_started_timeout_sec: u64,
    mut cmd_rx: mpsc::Receiver<TtsCommand>,
    audio_tx: mpsc::Sender<TtsAudio>,
) -> Result<()> {
    loop {
        if let Err(e) = run_tts_connection(
            url,
            api_key,
            model,
            voice,
            sample_rate,
            format,
            task_started_timeout_sec,
            &mut cmd_rx,
            &audio_tx,
        )
        .await
        {
            warn!(error = %e, "tts connection lost, reconnecting");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
}

async fn run_tts_connection(
    url: &str,
    api_key: &str,
    model: &str,
    voice: &str,
    sample_rate: u32,
    format: &str,
    task_started_timeout_sec: u64,
    cmd_rx: &mut mpsc::Receiver<TtsCommand>,
    audio_tx: &mpsc::Sender<TtsAudio>,
) -> Result<()> {
    let mut req = url
        .into_client_request()
        .map_err(|e| HalfDuplexError::WebSocket(e.to_string()))?;
    let auth = format!("bearer {api_key}");
    req.headers_mut().insert(
        "Authorization",
        auth.parse()
            .map_err(|e: http::header::InvalidHeaderValue| HalfDuplexError::WebSocket(e.to_string()))?,
    );

    let (ws, _) = connect_async(req)
        .await
        .map_err(|e| HalfDuplexError::WebSocket(e.to_string()))?;
    let (mut write, mut read) = ws.split();

    let mut task_id = String::new();
    let mut ready = false;
    let mut pending_finish: Option<oneshot::Sender<Result<()>>> = None;
    let mut pending_interrupt: Option<oneshot::Sender<Result<()>>> = None;

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    TtsCommand::WarmupStart(done) => {
                        pending_finish = None;
                        pending_interrupt = None;
                        let r = start_new_task(
                            &mut write,
                            &mut read,
                            model,
                            voice,
                            sample_rate,
                            format,
                            task_started_timeout_sec,
                            &mut task_id,
                            &mut ready,
                            &audio_tx,
                        )
                        .await;
                        let _ = done.send(r);
                    }
                    TtsCommand::InterruptTurn(done) => {
                        pending_interrupt = None;
                        if ready {
                            let fin = finish_task(&task_id);
                            if write.send(Message::Text(fin.to_string().into())).await.is_err() {
                                let _ = done.send(Err(HalfDuplexError::Tts("write finish-task failed".into())));
                            } else {
                                pending_interrupt = Some(done);
                            }
                        } else {
                            let _ = done.send(Ok(()));
                        }
                    }
                    TtsCommand::AppendText { text, done } => {
                        let r = async {
                            if !ready {
                                start_new_task(
                                    &mut write,
                                    &mut read,
                                    model,
                                    voice,
                                    sample_rate,
                                    format,
                                    task_started_timeout_sec,
                                    &mut task_id,
                                    &mut ready,
                                    &audio_tx,
                                )
                                .await?;
                            }
                            let msg = continue_task(&task_id, &text);
                            write.send(Message::Text(msg.to_string().into())).await
                                .map_err(|e| HalfDuplexError::Tts(e.to_string()))?;
                            Ok(())
                        }.await;
                        let _ = done.send(r);
                    }
                    TtsCommand::FinishTurn(done) => {
                        if ready {
                            let fin = finish_task(&task_id);
                            let _ = write.send(Message::Text(fin.to_string().into())).await;
                            pending_finish = Some(done);
                        } else {
                            let _ = done.send(Ok(()));
                        }
                    }
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        let _ = audio_tx.send(TtsAudio { pcm: b.to_vec() }).await;
                    }
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&t) {
                            match event_name(&v).as_deref() {
                                Some("task-started") => ready = true,
                                Some("task-finished") => {
                                    ready = false;
                                    if let Some(done) = pending_finish.take() {
                                        let _ = done.send(Ok(()));
                                    }
                                    if let Some(done) = pending_interrupt.take() {
                                        let _ = done.send(Ok(()));
                                    }
                                }
                                Some("task-failed") => {
                                    ready = false;
                                    let msg = dashscope::header_field(&v, "error_message")
                                        .unwrap_or_else(|| "task failed".into());
                                    let err = || HalfDuplexError::Tts(msg.clone());
                                    if let Some(done) = pending_finish.take() {
                                        let _ = done.send(Err(err()));
                                    }
                                    if let Some(done) = pending_interrupt.take() {
                                        let _ = done.send(Err(err()));
                                    }
                                    if msg.contains("timeout") {
                                        error!(
                                            error = %msg,
                                            "tts failed (check board can reach dashscope WSS and DNS; try raising tts.task_started_timeout_sec)"
                                        );
                                    } else {
                                        error!(error = %msg, "tts failed");
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => return Err(HalfDuplexError::Tts("closed".into())),
                    Some(Err(e)) => return Err(HalfDuplexError::Tts(e.to_string())),
                    None => return Err(HalfDuplexError::Tts("eof".into())),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

async fn start_new_task(
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    read: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    model: &str,
    voice: &str,
    sample_rate: u32,
    format: &str,
    task_started_timeout_sec: u64,
    task_id: &mut String,
    ready: &mut bool,
    audio_tx: &mpsc::Sender<TtsAudio>,
) -> Result<()> {
    *task_id = dashscope::task_id();
    *ready = false;
    let run = run_task_tts(task_id, model, voice, sample_rate, format);
    write
        .send(Message::Text(run.to_string().into()))
        .await
        .map_err(|e| HalfDuplexError::Tts(e.to_string()))?;

    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(task_started_timeout_sec.max(5));
    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(v) = serde_json::from_str(&t) {
                            if event_name(&v).as_deref() == Some("task-started") {
                                *ready = true;
                                return Ok(());
                            }
                            if event_name(&v).as_deref() == Some("task-failed") {
                                return Err(HalfDuplexError::Tts(
                                    dashscope::header_field(&v, "error_message")
                                        .unwrap_or_else(|| "task failed".into()),
                                ));
                            }
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        let _ = audio_tx.send(TtsAudio { pcm: b.to_vec() }).await;
                    }
                    Some(Err(e)) => return Err(HalfDuplexError::Tts(e.to_string())),
                    _ => {}
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
    }
    Err(HalfDuplexError::Tts("task-started timeout".into()))
}
