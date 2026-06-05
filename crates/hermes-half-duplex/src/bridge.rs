//! Shared bridge between the voice session loop and the Hermes TUI agent.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::warn;

use crate::audio::AudioPlayback;
use crate::busy_replies::BusyReplyGate;
use crate::config::OrchestratorConfig;
use crate::error::Result;
use crate::orchestrator::{flush_remainder, take_early_chunk, take_sentence};
use crate::tts::TtsClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceChatStatus {
    /// Wake word enabled; mic hot, ASR paused until KWS fires.
    WaitingWake,
    Listening,
    Thinking,
    Speaking,
}

#[derive(Debug, Clone)]
pub enum VoiceChatEvent {
    UserUtterance(String),
    /// User spoke while agent was busy; interrupt current turn and handle this text.
    InterruptForUtterance(String),
    BusyReply { phrase: String, heard: String },
    TurnComplete,
    /// Wake word accepted; user may speak their query.
    WakeAccepted,
    /// User barged in during TTS/thinking; TUI should cancel the in-flight agent task.
    BargeIn,
    Status(VoiceChatStatus),
    Error(String),
}

pub struct VoiceChatBridge {
    pub agent_busy: AtomicBool,
    /// True while assistant TTS is being synthesized or played out.
    playout_active: AtomicBool,
    pub utterance_tx: mpsc::Sender<VoiceChatEvent>,
    playback: Arc<AudioPlayback>,
    play_gen: Arc<AtomicU64>,
    tts: TtsClient,
    orch: OrchestratorConfig,
    assistant_buf: Mutex<String>,
    tts_buf: Mutex<String>,
    sent_early: Mutex<bool>,
    pending_utterance: Mutex<Option<String>>,
    busy_gate: Mutex<BusyReplyGate>,
    status: Mutex<VoiceChatStatus>,
}

impl VoiceChatBridge {
    pub fn new(
        tts: TtsClient,
        orch: OrchestratorConfig,
        utterance_tx: mpsc::Sender<VoiceChatEvent>,
        busy_cooldown_secs: u64,
        playback: Arc<AudioPlayback>,
        play_gen: Arc<AtomicU64>,
    ) -> Arc<Self> {
        Arc::new(Self {
            agent_busy: AtomicBool::new(false),
            playout_active: AtomicBool::new(false),
            utterance_tx,
            playback,
            play_gen,
            tts,
            orch,
            assistant_buf: Mutex::new(String::new()),
            tts_buf: Mutex::new(String::new()),
            sent_early: Mutex::new(false),
            pending_utterance: Mutex::new(None),
            busy_gate: Mutex::new(BusyReplyGate::new(busy_cooldown_secs)),
            status: Mutex::new(VoiceChatStatus::Listening),
        })
    }

    pub async fn set_status(self: &Arc<Self>, st: VoiceChatStatus) {
        *self.status.lock().await = st;
        let _ = self.utterance_tx.send(VoiceChatEvent::Status(st)).await;
    }

    pub fn is_agent_busy(&self) -> bool {
        self.agent_busy.load(Ordering::SeqCst)
    }

    pub fn is_playout_active(&self) -> bool {
        self.playout_active.load(Ordering::SeqCst)
    }

    /// Agent running and/or TTS audio still playing/synthesizing.
    pub fn is_output_busy(&self, playback_buffered: usize, sample_rate: u32) -> bool {
        self.is_agent_busy()
            || self.is_playout_active()
            || playback_buffered > sample_rate as usize / 10
    }

    pub fn playback(&self) -> &Arc<AudioPlayback> {
        &self.playback
    }

    /// Halt speaker output synchronously (must be instant for barge-in).
    pub fn halt_playback_sync(&self) {
        self.playback.halt_playout(&self.play_gen);
    }

    /// Prepare speaker for a new TTS turn after barge-in or at turn start.
    pub fn begin_playback_sync(&self) {
        self.playback.begin_playout(&self.play_gen);
    }

    pub async fn warmup(&self) -> Result<()> {
        self.tts.warmup().await
    }

    pub fn stop_playback_now(&self) {
        self.halt_playback_sync();
    }

    pub async fn notify_wake_accepted(self: &Arc<Self>) {
        let _ = self
            .utterance_tx
            .send(VoiceChatEvent::WakeAccepted)
            .await;
    }

    pub async fn send_error(&self, message: String) {
        let _ = self
            .utterance_tx
            .send(VoiceChatEvent::Error(message))
            .await;
    }

    pub async fn queue_pending_utterance(&self, text: String) {
        *self.pending_utterance.lock().await = Some(text);
    }

    pub async fn on_asr_final_request_interrupt(self: &Arc<Self>, text: String) {
        let _ = self
            .utterance_tx
            .send(VoiceChatEvent::InterruptForUtterance(text))
            .await;
    }

    /// Stop speaker immediately; interrupt TTS websocket in the background.
    pub fn barge_in_immediate(self: &Arc<Self>) {
        self.playout_active.store(false, Ordering::SeqCst);
        self.halt_playback_sync();
        let this = self.clone();
        tokio::spawn(async move {
            this.reset_assistant_buffers().await;
            if let Err(e) = this.tts.interrupt_turn().await {
                warn!(error = %e, "tts barge-in failed");
            }
            let _ = this.set_status(VoiceChatStatus::Listening).await;
        });
    }

    /// Tell the TUI to cancel an in-flight LLM turn after barge-in.
    pub fn signal_agent_barge_in(self: &Arc<Self>) {
        if !self.is_agent_busy() {
            return;
        }
        let tx = self.utterance_tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(VoiceChatEvent::BargeIn).await;
        });
    }

    /// Stop speaker output and in-flight TTS synthesis (barge-in).
    pub async fn barge_in_speech(self: &Arc<Self>) {
        self.barge_in_immediate();
        if let Err(e) = self.tts.interrupt_turn().await {
            warn!(error = %e, "tts barge-in failed");
        }
        let _ = self.set_status(VoiceChatStatus::Listening).await;
    }

    /// Stop in-flight TTS/LLM turn without flushing assistant text to speech.
    pub async fn abort_agent_turn(self: &Arc<Self>) {
        self.playout_active.store(false, Ordering::SeqCst);
        self.barge_in_immediate();
        *self.pending_utterance.lock().await = None;
        self.agent_busy.store(false, Ordering::SeqCst);
    }

    pub fn abort_agent_turn_fast(self: &Arc<Self>) {
        self.playout_active.store(false, Ordering::SeqCst);
        self.barge_in_immediate();
        self.agent_busy.store(false, Ordering::SeqCst);
    }

    pub async fn on_asr_final_while_busy(self: &Arc<Self>, text: String) {
        self.queue_pending_utterance(text.clone()).await;
        let mut gate = self.busy_gate.lock().await;
        if !gate.should_play() {
            let _ = self
                .utterance_tx
                .send(VoiceChatEvent::BusyReply {
                    phrase: String::new(),
                    heard: text,
                })
                .await;
            return;
        }
        let phrase = gate.pick();
        gate.mark_played();
        drop(gate);
        if let Err(e) = self.speak_line(phrase).await {
            warn!(error = %e, "busy reply tts failed");
        }
        let _ = self
            .utterance_tx
            .send(VoiceChatEvent::BusyReply {
                phrase: phrase.to_string(),
                heard: text,
            })
            .await;
    }

    pub async fn trigger_user_utterance(self: &Arc<Self>, text: String) -> Result<()> {
        self.agent_busy.store(true, Ordering::SeqCst);
        self.reset_assistant_buffers().await;
        self.set_status(VoiceChatStatus::Thinking).await;
        self.utterance_tx
            .send(VoiceChatEvent::UserUtterance(text))
            .await
            .map_err(|e| crate::error::HalfDuplexError::Other(anyhow::anyhow!(e)))?;
        Ok(())
    }

    pub async fn reset_assistant_buffers(&self) {
        self.assistant_buf.lock().await.clear();
        self.tts_buf.lock().await.clear();
        *self.sent_early.lock().await = false;
    }

    pub async fn feed_assistant_delta(self: &Arc<Self>, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.assistant_buf.lock().await.push_str(delta);
        let mut buf = self.tts_buf.lock().await;
        buf.push_str(delta);
        let orch = &self.orch;
        let mut sent_early = self.sent_early.lock().await;
        if !*sent_early {
            if let Some(chunk) = take_early_chunk(&mut buf, orch.tts_first_chunk_chars) {
                drop(sent_early);
                self.begin_playback_sync();
                if self.tts.append_text(chunk).await.is_err() {
                    return;
                }
                *self.sent_early.lock().await = true;
                self.playout_active.store(true, Ordering::SeqCst);
                self.set_status(VoiceChatStatus::Speaking).await;
                sent_early = self.sent_early.lock().await;
            }
        }
        while let Some(sentence) = take_sentence(&mut buf, orch.sentence_min_len) {
            if !*sent_early {
                drop(sent_early);
                self.begin_playback_sync();
                *self.sent_early.lock().await = true;
                sent_early = self.sent_early.lock().await;
            }
            let sentence = sentence;
            drop(sent_early);
            let _ = self.tts.append_text(sentence).await;
            self.playout_active.store(true, Ordering::SeqCst);
            self.set_status(VoiceChatStatus::Speaking).await;
            sent_early = self.sent_early.lock().await;
        }
    }

    pub async fn complete_agent_turn(self: &Arc<Self>) {
        let mut buf = self.tts_buf.lock().await;
        if let Some(rest) = flush_remainder(&mut buf) {
            if !self.playout_active.load(Ordering::SeqCst) {
                self.begin_playback_sync();
            }
            self.playout_active.store(true, Ordering::SeqCst);
            let _ = self.tts.append_text(rest).await;
        }
        if let Err(e) = self.tts.finish_turn_async().await {
            warn!(error = %e, "tts finish failed");
        }
        self.agent_busy.store(false, Ordering::SeqCst);
        // playout_active stays true until playback drains or user barges in.
        let _ = self.set_status(VoiceChatStatus::Listening).await;
        let _ = self.utterance_tx.send(VoiceChatEvent::TurnComplete).await;

        let pending = self.pending_utterance.lock().await.take();
        if let Some(text) = pending {
            if let Err(e) = self.trigger_user_utterance(text).await {
                warn!(error = %e, "failed to drain pending utterance");
            }
        }
    }

    pub async fn speak_line(self: &Arc<Self>, line: &str) -> Result<()> {
        self.begin_playback_sync();
        self.playout_active.store(true, Ordering::SeqCst);
        let _ = self.tts.append_text(line.to_string()).await?;
        let _ = self.tts.finish_turn_async().await?;
        Ok(())
    }

    /// Mark playout finished once the speaker queue has drained.
    pub fn clear_playout_if_drained(self: &Arc<Self>) {
        if self.playback.buffered_samples() < 480 {
            self.playout_active.store(false, Ordering::SeqCst);
        }
    }
}
