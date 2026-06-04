use webrtc_vad::{Vad, VadMode};

use super::EndpointDetector;

const FRAME_MS: u32 = 30;

pub struct WebRtcVad {
    vad: Vad,
    sample_rate: u32,
    frame_samples: usize,
    pending: Vec<i16>,
    in_speech: bool,
    trailing_silence_ms: u32,
    speech_start_flag: bool,
    speech_frames: u32,
    barge_in_threshold: u32,
}

impl WebRtcVad {
    pub fn new(sample_rate: u32, barge_in_frames: u32) -> Self {
        let mut vad = Vad::new();
        vad.set_mode(VadMode::Aggressive);
        let frame_samples = (sample_rate as u64 * FRAME_MS as u64 / 1000) as usize;
        Self {
            vad,
            sample_rate,
            frame_samples,
            pending: Vec::new(),
            in_speech: false,
            trailing_silence_ms: 0,
            speech_start_flag: false,
            speech_frames: 0,
            barge_in_threshold: barge_in_frames.max(1),
        }
    }
}

impl EndpointDetector for WebRtcVad {
    fn feed(&mut self, samples: &[f32]) {
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            self.pending.push(v);
        }
        while self.pending.len() >= self.frame_samples {
            let frame: Vec<i16> = self.pending.drain(..self.frame_samples).collect();
            let voice = self
                .vad
                .is_voice_segment(&frame)
                .unwrap_or(false);

            if voice {
                if !self.in_speech {
                    self.speech_frames += 1;
                    if self.speech_frames >= self.barge_in_threshold {
                        self.speech_start_flag = true;
                        self.in_speech = true;
                        self.speech_frames = 0;
                    }
                } else {
                    self.speech_frames = 0;
                }
                self.trailing_silence_ms = 0;
            } else {
                self.speech_frames = 0;
                if self.in_speech {
                    self.trailing_silence_ms = self.trailing_silence_ms.saturating_add(FRAME_MS);
                } else {
                    self.trailing_silence_ms =
                        self.trailing_silence_ms.saturating_add(FRAME_MS);
                }
                if self.trailing_silence_ms > FRAME_MS * 2 {
                    self.in_speech = false;
                }
            }
        }
        let _ = self.sample_rate;
    }

    fn trailing_silence_ms(&self) -> u32 {
        self.trailing_silence_ms
    }

    fn speech_start(&mut self) -> bool {
        if self.speech_start_flag {
            self.speech_start_flag = false;
            return true;
        }
        false
    }

    fn in_speech(&self) -> bool {
        self.in_speech
    }
}

impl WebRtcVad {
    /// Reset speech state after barge-in so the next utterance can trigger again.
    pub fn reset_barge_in_state(&mut self) {
        self.in_speech = false;
        self.trailing_silence_ms = 0;
        self.speech_start_flag = false;
        self.speech_frames = 0;
    }

    /// True if user is speaking during assistant playback (not only on edge).
    pub fn user_speaking_during_playback(&self) -> bool {
        self.in_speech && self.speech_frames == 0
    }
}
