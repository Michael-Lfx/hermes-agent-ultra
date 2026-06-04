//! Voice chat session loop (mic → ASR → Hermes agent via bridge → TTS).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::asr::{AsrClient, AsrEvent};
use crate::audio::{AudioCapture, AudioPlayback};
use crate::bridge::{VoiceChatBridge, VoiceChatStatus};
use crate::config::Config;
use crate::error::Result;
use crate::kws::{start_wake_detector, WakeDetectorHandle};
use crate::orchestrator::state::SessionState;
use crate::orchestrator::wake::WakePhase;
use crate::tts::TtsAudio;
use crate::vad::{EndpointDetector, WebRtcVad};

pub struct VoiceChatSession {
    cfg: Config,
    bridge: Arc<VoiceChatBridge>,
    cancel: CancellationToken,
    tts_rx: mpsc::Receiver<TtsAudio>,
    playback: Arc<AudioPlayback>,
    play_gen: Arc<AtomicU64>,
}

impl VoiceChatSession {
    pub fn new(
        cfg: Config,
        bridge: Arc<VoiceChatBridge>,
        cancel: CancellationToken,
        tts_rx: mpsc::Receiver<TtsAudio>,
        playback: Arc<AudioPlayback>,
        play_gen: Arc<AtomicU64>,
    ) -> Self {
        Self {
            cfg,
            bridge,
            cancel,
            tts_rx,
            playback,
            play_gen,
        }
    }

    pub async fn run(self) -> Result<()> {
        let orch = self.cfg.orchestrator.clone();
        let wake_cfg = self.cfg.wake.clone();
        let wake_enabled = wake_cfg.enabled;

        let capture = AudioCapture::start(&self.cfg.audio, self.cfg.asr.chunk_ms)?;
        let playback = self.playback;
        let _play_gen = self.play_gen;

        let (asr, mut asr_rx) =
            AsrClient::connect(&self.cfg.dashscope, &self.cfg.asr, wake_enabled).await?;

        let wake_detector: Option<WakeDetectorHandle> = if wake_enabled {
            Some(start_wake_detector(&wake_cfg, self.cfg.asr.sample_rate)?)
        } else {
            None
        };

        let mut wake_phase = if wake_enabled {
            let _ = asr.pause().await;
            WakePhase::Dormant
        } else {
            asr.resume().await?;
            wait_asr_started(&mut asr_rx).await;
            WakePhase::Active
        };

        self.bridge.warmup().await?;
        let mut tts_rx = self.tts_rx;

        let (pcm_tx, mut pcm_rx) = mpsc::channel(64);
        std::thread::spawn(move || loop {
            if let Some(chunk) = capture.try_recv_chunk() {
                let _ = pcm_tx.blocking_send(chunk);
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        });

        let playback_tts = playback.clone();
        let play_gen_tts = _play_gen.clone();
        tokio::spawn(async move {
            while let Some(audio) = tts_rx.recv().await {
                // After barge-in the queue is halted; drop stale websocket PCM until
                // begin_playout() opens the next generation.
                if playback_tts.is_stopped() {
                    continue;
                }
                let g = play_gen_tts.load(Ordering::SeqCst);
                playback_tts.enqueue_pcm_i16(g, &audio.pcm);
            }
        });

        let mut vad = WebRtcVad::new(self.cfg.asr.sample_rate, orch.barge_in_frames);
        let mut state = SessionState::Listening;
        let session_start = Instant::now();
        let cold_start = Duration::from_secs(orch.cold_start_sec);
        let grace_after_wake = Duration::from_secs(wake_cfg.grace_after_wake_sec);
        let idle_after_turn = Duration::from_secs(wake_cfg.idle_after_turn_sec);

        let mut last_final: Option<String> = None;
        let mut asr_final_at: Option<Instant> = None;

        let bridge = self.bridge.clone();
        bridge.set_status(VoiceChatStatus::Listening).await;

        info!(
            wake_enabled,
            phrases = ?wake_cfg.effective_phrases(),
            "voice chat session ready"
        );
        if wake_enabled {
            info!("waiting for wake word before dialog");
        }

        loop {
            if self.cancel.is_cancelled() {
                break;
            }

            bridge.clear_playout_if_drained();

            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => break,
                chunk = pcm_rx.recv() => {
                    let Some(chunk) = chunk else { break };
                    if let Some(ref det) = wake_detector {
                        det.feed(&chunk.samples_f32);
                    }
                    vad.feed(&chunk.samples_f32);

                    if try_barge_in(
                        "vad",
                        &orch,
                        &mut state,
                        &mut vad,
                        &playback,
                        &bridge,
                        None,
                    ) {
                        continue;
                    }

                    if wake_enabled && matches!(wake_phase, WakePhase::Dormant) {
                        if wake_detector.as_ref().is_some_and(|d| d.try_recv_wake()) {
                            if let Err(e) = asr.resume().await {
                                warn!(error = %e, "asr resume after wake failed");
                            } else {
                                wait_asr_started(&mut asr_rx).await;
                                let ack_extra = if wake_cfg.ack_reply.trim().is_empty() {
                                    Duration::ZERO
                                } else {
                                    Duration::from_secs(3)
                                };
                                wake_phase = WakePhase::AwakeGrace {
                                    deadline: Instant::now() + grace_after_wake + ack_extra,
                                };
                                info!(grace_sec = wake_cfg.grace_after_wake_sec, "wake accepted");
                                bridge.notify_wake_accepted().await;
                                if !wake_cfg.ack_reply.trim().is_empty() {
                                    let ack = wake_cfg.ack_reply.clone();
                                    let bridge_ack = bridge.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = bridge_ack.speak_line(&ack).await {
                                            warn!(error = %e, "wake ack tts failed");
                                        }
                                    });
                                }
                            }
                        }
                    }

                    if wake_phase.allows_asr() {
                        let _ = asr.send_audio(chunk.samples_i16_bytes).await;
                    }

                    if user_speech_activity(&mut vad, None, orch.min_final_chars, &wake_phase) {
                        wake_phase = promote_wake_on_speech(wake_phase);
                    }

                    if wake_phase.check_timeout(Instant::now()) {
                        enter_dormant(
                            &asr,
                            &mut wake_phase,
                            &mut state,
                            &mut last_final,
                            &mut asr_final_at,
                            &mut asr_rx,
                        )
                        .await;
                        continue;
                    }

                    if !wake_phase.allows_dialog() {
                        continue;
                    }
                }
                ev = asr_rx.recv() => {
                    let Some(ev) = ev else { continue };
                    match ev {
                        AsrEvent::Partial { text } => {
                            if user_speech_activity(
                                &mut vad,
                                Some(&text),
                                orch.min_final_chars,
                                &wake_phase,
                            ) {
                                wake_phase = promote_wake_on_speech(wake_phase);
                            }
                            if !wake_phase.allows_dialog() {
                                continue;
                            }
                            if try_barge_in(
                                "asr-partial",
                                &orch,
                                &mut state,
                                &mut vad,
                                &playback,
                                &bridge,
                                Some(text.as_str()),
                            ) {
                                continue;
                            }
                        }
                        AsrEvent::Final { text } => {
                            if text.trim().chars().count() < orch.min_final_chars {
                                continue;
                            }
                            if user_speech_activity(
                                &mut vad,
                                Some(&text),
                                orch.min_final_chars,
                                &wake_phase,
                            ) {
                                wake_phase = promote_wake_on_speech(wake_phase);
                            }
                            if !wake_phase.allows_dialog() {
                                continue;
                            }
                            if bridge.is_agent_busy() {
                                if self.cfg.busy_replies.enabled {
                                    bridge.on_asr_final_while_busy(text).await;
                                } else {
                                    try_barge_in(
                                        "asr-final-busy",
                                        &orch,
                                        &mut state,
                                        &mut vad,
                                        &playback,
                                        &bridge,
                                        Some(text.as_str()),
                                    );
                                    bridge.on_asr_final_request_interrupt(text).await;
                                }
                                last_final = None;
                                continue;
                            }
                            if bridge.is_output_busy(
                                playback.buffered_samples(),
                                playback.sample_rate(),
                            ) {
                                try_barge_in(
                                    "asr-final-playout",
                                    &orch,
                                    &mut state,
                                    &mut vad,
                                    &playback,
                                    &bridge,
                                    Some(text.as_str()),
                                );
                                bridge.on_asr_final_request_interrupt(text).await;
                                last_final = None;
                                continue;
                            }
                            last_final = Some(text);
                            asr_final_at = Some(Instant::now());
                            maybe_trigger(
                                &orch,
                                &mut state,
                                session_start,
                                cold_start,
                                &mut vad,
                                &mut last_final,
                                &mut asr_final_at,
                                &bridge,
                                &playback,
                            )
                            .await;
                        }
                        AsrEvent::TaskFailed { message } => warn!(%message, "asr failed"),
                        _ => {}
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    bridge.clear_playout_if_drained();
                    if state == SessionState::Thinking && !bridge.is_agent_busy() {
                        state = SessionState::Listening;
                        if wake_enabled {
                            wake_phase = WakePhase::IdleAfterTurn {
                                deadline: Instant::now() + idle_after_turn,
                            };
                            info!(
                                idle_sec = wake_cfg.idle_after_turn_sec,
                                "turn done; idle timeout before dormant"
                            );
                        }
                    }
                    if state == SessionState::Listening && wake_phase.allows_dialog() {
                        maybe_trigger(
                            &orch,
                            &mut state,
                            session_start,
                            cold_start,
                            &mut vad,
                            &mut last_final,
                            &mut asr_final_at,
                            &bridge,
                            &playback,
                        )
                        .await;
                    }
                }
            }
        }

        Ok(())
    }
}

fn is_output_busy(bridge: &VoiceChatBridge, playback: &AudioPlayback) -> bool {
    bridge.is_output_busy(playback.buffered_samples(), playback.sample_rate())
}

fn asr_indicates_barge_in(text: &str, min_chars: usize) -> bool {
    text.trim().chars().count() >= min_chars
}

fn try_barge_in(
    reason: &str,
    _orch: &crate::config::OrchestratorConfig,
    state: &mut SessionState,
    vad: &mut WebRtcVad,
    playback: &AudioPlayback,
    bridge: &Arc<VoiceChatBridge>,
    asr_text: Option<&str>,
) -> bool {
    if !is_output_busy(bridge, playback) {
        return false;
    }

    let barge_min = 1usize;
    let vad_hit = vad.speech_start()
        || vad.user_speaking_during_playback()
        || (asr_text.is_none() && vad.in_speech());
    let asr_hit = asr_text
        .map(|t| asr_indicates_barge_in(t, barge_min))
        .unwrap_or(false);

    if !vad_hit && !asr_hit {
        return false;
    }

    info!(reason, vad_hit, asr_hit, "barge-in");
    bridge.barge_in_immediate();
    bridge.signal_agent_barge_in();
    vad.reset_barge_in_state();
    *state = SessionState::Listening;
    true
}

fn user_speech_activity(
    vad: &mut WebRtcVad,
    text: Option<&str>,
    min_chars: usize,
    wake_phase: &WakePhase,
) -> bool {
    if vad.speech_start() || vad.in_speech() {
        return true;
    }
    let min = match *wake_phase {
        WakePhase::AwakeGrace { .. } | WakePhase::IdleAfterTurn { .. } => 1,
        _ => min_chars,
    };
    text.is_some_and(|t| t.trim().chars().count() >= min)
}

fn promote_wake_on_speech(wake: WakePhase) -> WakePhase {
    match wake {
        WakePhase::AwakeGrace { .. } => {
            info!("wake grace -> active (user speech)");
            WakePhase::Active
        }
        WakePhase::IdleAfterTurn { .. } => {
            info!("idle after turn -> active (user speech)");
            WakePhase::Active
        }
        other => other,
    }
}

async fn enter_dormant(
    asr: &AsrClient,
    wake_phase: &mut WakePhase,
    state: &mut SessionState,
    last_final: &mut Option<String>,
    asr_final_at: &mut Option<Instant>,
    asr_rx: &mut mpsc::Receiver<AsrEvent>,
) {
    let _ = asr.pause().await;
    let mut drained = 0usize;
    while asr_rx.try_recv().is_ok() {
        drained += 1;
    }
    *wake_phase = WakePhase::Dormant;
    *state = SessionState::Listening;
    *last_final = None;
    *asr_final_at = None;
    info!(
        drained_asr_events = drained,
        "wake timeout -> dormant; say wake word again"
    );
}

async fn wait_asr_started(asr_rx: &mut mpsc::Receiver<AsrEvent>) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Some(ev) = asr_rx.recv().await {
            if matches!(ev, AsrEvent::TaskStarted) {
                return;
            }
        }
    }
}

async fn maybe_trigger(
    orch: &crate::config::OrchestratorConfig,
    state: &mut SessionState,
    session_start: Instant,
    cold_start: Duration,
    vad: &mut WebRtcVad,
    last_final: &mut Option<String>,
    asr_final_at: &mut Option<Instant>,
    bridge: &Arc<VoiceChatBridge>,
    playback: &Arc<AudioPlayback>,
) {
    if *state != SessionState::Listening || bridge.is_agent_busy() {
        return;
    }
    if is_output_busy(bridge, playback) {
        return;
    }
    if session_start.elapsed() < cold_start {
        return;
    }
    if vad.trailing_silence_ms() < orch.endpoint_silence_ms() {
        return;
    }
    let Some(text) = last_final.take() else {
        return;
    };
    if text.trim().chars().count() < orch.min_final_chars {
        return;
    }
    let _ = asr_final_at.take();

    if let Err(e) = bridge.trigger_user_utterance(text).await {
        warn!(error = %e, "trigger user utterance failed");
        *state = SessionState::Listening;
        bridge.agent_busy.store(false, Ordering::SeqCst);
        return;
    }
    *state = SessionState::Thinking;
}
