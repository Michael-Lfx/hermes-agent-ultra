use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tracing::error;

/// Reports microphone/speaker stream faults once (cpal may call the error callback repeatedly).
#[derive(Default)]
pub struct AudioFault {
    capture_lost: AtomicBool,
    playback_lost: AtomicBool,
    pending: Mutex<Vec<String>>,
}

impl AudioFault {
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn report_capture(self: &Arc<Self>, err: cpal::StreamError) {
        if self.capture_lost.swap(true, Ordering::Relaxed) {
            return;
        }
        error!(error = %err, "microphone stream lost");
        self.pending.lock().unwrap().push(format!(
            "Microphone unavailable ({err}). Re-plug the device or set audio.input_device in half_duplex.toml, then /stop-chat and /chat."
        ));
    }

    pub fn report_playback(self: &Arc<Self>, err: cpal::StreamError) {
        if self.playback_lost.swap(true, Ordering::Relaxed) {
            return;
        }
        error!(error = %err, "speaker stream lost");
        self.pending.lock().unwrap().push(format!(
            "Speaker unavailable ({err}). Re-plug the device or set audio.output_device in half_duplex.toml, then /stop-chat and /chat."
        ));
    }

    pub fn drain_messages(&self) -> Vec<String> {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }
}
