use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::asr::AsrEvent;
use crate::config::{AsrConfig, DashscopeConfig};
use crate::dashscope::{self, event_name, finish_task, run_task_asr};
use crate::error::{HalfDuplexError, Result};

enum AsrCommand {
    Audio(Vec<u8>),
    Pause(oneshot::Sender<Result<()>>),
    Resume(oneshot::Sender<Result<()>>),
}

pub struct AsrClient {
    cmd_tx: mpsc::Sender<AsrCommand>,
}

impl AsrClient {
    pub async fn connect(
        dashscope: &DashscopeConfig,
        asr: &AsrConfig,
        start_paused: bool,
    ) -> Result<(Self, mpsc::Receiver<AsrEvent>)> {
        let (event_tx, event_rx) = mpsc::channel(64);
        let (cmd_tx, cmd_rx) = mpsc::channel::<AsrCommand>(128);

        let url = dashscope.ws_url.clone();
        let api_key = dashscope.api_key.clone();
        let model = asr.model.clone();
        let sample_rate = asr.sample_rate;
        let format = asr.format.clone();

        tokio::spawn(async move {
            if let Err(e) = run_asr_loop(
                &url,
                &api_key,
                &model,
                sample_rate,
                &format,
                cmd_rx,
                &event_tx,
                start_paused,
            )
            .await
            {
                error!(error = %e, "asr loop ended");
                let _ = event_tx
                    .send(AsrEvent::TaskFailed {
                        message: e.to_string(),
                    })
                    .await;
            }
        });

        Ok((Self { cmd_tx }, event_rx))
    }

    pub async fn send_audio(&self, pcm: Vec<u8>) -> Result<()> {
        self.cmd_tx
            .send(AsrCommand::Audio(pcm))
            .await
            .map_err(|e| HalfDuplexError::Asr(format!("send audio: {e}")))
    }

    pub async fn pause(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(AsrCommand::Pause(tx))
            .await
            .map_err(|e| HalfDuplexError::Asr(format!("pause: {e}")))?;
        rx.await
            .map_err(|e| HalfDuplexError::Asr(format!("pause response: {e}")))?
    }

    pub async fn resume(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(AsrCommand::Resume(tx))
            .await
            .map_err(|e| HalfDuplexError::Asr(format!("resume: {e}")))?;
        rx.await
            .map_err(|e| HalfDuplexError::Asr(format!("resume response: {e}")))?
    }
}

async fn run_asr_loop(
    url: &str,
    api_key: &str,
    model: &str,
    sample_rate: u32,
    format: &str,
    mut cmd_rx: mpsc::Receiver<AsrCommand>,
    event_tx: &mpsc::Sender<AsrEvent>,
    start_paused: bool,
) -> Result<()> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match run_asr_session(
            url,
            api_key,
            model,
            sample_rate,
            format,
            &mut cmd_rx,
            event_tx,
            start_paused && attempt == 1,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(e) if attempt < 2 => {
                warn!(error = %e, attempt, "asr reconnecting");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn run_asr_session(
    url: &str,
    api_key: &str,
    model: &str,
    sample_rate: u32,
    format: &str,
    cmd_rx: &mut mpsc::Receiver<AsrCommand>,
    event_tx: &mpsc::Sender<AsrEvent>,
    initial_paused: bool,
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

    let mut task_id = dashscope::task_id();
    let mut started = false;
    let mut paused = initial_paused;
    let resume_waiters: Arc<tokio::sync::Mutex<Vec<oneshot::Sender<Result<()>>>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));

    async fn start_task(
        write: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        task_id: &mut String,
        model: &str,
        sample_rate: u32,
        format: &str,
    ) -> Result<()> {
        *task_id = dashscope::task_id();
        let run = run_task_asr(task_id, model, sample_rate, format);
        write
            .send(Message::Text(run.to_string().into()))
            .await
            .map_err(|e| HalfDuplexError::Asr(e.to_string()))?;
        Ok(())
    }

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(AsrCommand::Audio(bytes)) => {
                        if started && !paused {
                            write.send(Message::Binary(bytes.into())).await
                                .map_err(|e| HalfDuplexError::Asr(e.to_string()))?;
                        }
                    }
                    Some(AsrCommand::Pause(done)) => {
                        if started {
                            let fin = finish_task(&task_id);
                            let _ = write.send(Message::Text(fin.to_string().into())).await;
                            started = false;
                            info!("asr paused");
                        }
                        paused = true;
                        let _ = done.send(Ok(()));
                    }
                    Some(AsrCommand::Resume(done)) => {
                        paused = false;
                        if !started {
                            if let Err(e) = start_task(&mut write, &mut task_id, model, sample_rate, format).await {
                                let _ = done.send(Err(e));
                            } else {
                                resume_waiters.lock().await.push(done);
                            }
                        } else {
                            let _ = done.send(Ok(()));
                        }
                    }
                    None => break,
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        let v: Value = serde_json::from_str(&t)
                            .map_err(|e| HalfDuplexError::Asr(e.to_string()))?;
                        if let Some(ev) = parse_asr_event(&v) {
                            if matches!(ev, AsrEvent::TaskStarted) {
                                started = true;
                                let mut waiters = resume_waiters.lock().await;
                                for w in waiters.drain(..) {
                                    let _ = w.send(Ok(()));
                                }
                            }
                            let _ = event_tx.send(ev).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Err(e)) => return Err(HalfDuplexError::Asr(e.to_string())),
                    None => break,
                    _ => {}
                }
            }
        }
    }

    if started {
        let fin = finish_task(&task_id);
        let _ = write
            .send(Message::Text(fin.to_string().into()))
            .await;
    }
    info!("asr session closed");
    Ok(())
}

fn parse_asr_event(msg: &Value) -> Option<AsrEvent> {
    let event = event_name(msg)?;
    match event.as_str() {
        "task-started" => Some(AsrEvent::TaskStarted),
        "task-failed" => {
            let message = dashscope::header_field(msg, "error_message")
                .unwrap_or_else(|| "unknown".into());
            Some(AsrEvent::TaskFailed { message })
        }
        "result-generated" => {
            let sentence = msg
                .get("payload")?
                .get("output")?
                .get("sentence")?;
            if sentence.get("heartbeat").and_then(|v| v.as_bool()) == Some(true) {
                return None;
            }
            let text = sentence.get("text")?.as_str()?.to_string();
            if text.is_empty() {
                return None;
            }
            let sentence_end = sentence
                .get("sentence_end")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if sentence_end {
                Some(AsrEvent::Final { text })
            } else {
                Some(AsrEvent::Partial { text })
            }
        }
        _ => None,
    }
}
