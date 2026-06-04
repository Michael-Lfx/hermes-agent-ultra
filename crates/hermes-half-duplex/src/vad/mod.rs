mod webrtc_vad;

pub use webrtc_vad::WebRtcVad;

pub trait EndpointDetector {
    fn feed(&mut self, samples: &[f32]);
    fn trailing_silence_ms(&self) -> u32;
    fn speech_start(&mut self) -> bool;
    fn in_speech(&self) -> bool;
    fn user_speaking_during_playback(&self) -> bool {
        self.in_speech()
    }
    fn reset_barge_in_state(&mut self) {}
}
