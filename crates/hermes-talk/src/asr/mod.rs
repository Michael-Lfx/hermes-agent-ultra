mod bailian;
#[cfg(feature = "sherpa-asr-tts")]
mod sherpa_asr;
mod types;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::backends::{TalkBackendKind, classify_talk_backend};
use crate::config::{AsrConfig, DashscopeConfig};
use crate::error::Result;

pub use bailian::BailianAsr;
pub use types::AsrEvent;

#[cfg(feature = "sherpa-asr-tts")]
pub use sherpa_asr::SherpaAsr;

#[async_trait]
pub trait AsrEngine: Send + Sync {
    async fn send_audio(&self, pcm: Vec<u8>) -> Result<()>;
    async fn pause(&self) -> Result<()>;
    async fn resume(&self) -> Result<()>;
    async fn set_gate(&self, on: bool) -> Result<()>;
    async fn reconnect(&self) -> Result<()>;
    async fn finish_utterance(&self) -> Result<()>;
    async fn begin_utterance(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsrBackend {
    Bailian,
    #[cfg(feature = "sherpa-asr-tts")]
    Sherpa,
}

impl AsrBackend {
    pub fn from_config(asr_cfg: &AsrConfig) -> Self {
        match classify_talk_backend(&asr_cfg.backend) {
            TalkBackendKind::Cloud => AsrBackend::Bailian,
            TalkBackendKind::Sherpa | TalkBackendKind::LocalHardware => resolve_local_sherpa_asr(),
        }
    }
}

#[cfg(feature = "sherpa-asr-tts")]
fn resolve_local_sherpa_asr() -> AsrBackend {
    AsrBackend::Sherpa
}

#[cfg(not(feature = "sherpa-asr-tts"))]
fn resolve_local_sherpa_asr() -> AsrBackend {
    AsrBackend::Bailian
}

pub async fn create_asr(
    dashscope: &DashscopeConfig,
    asr_cfg: &AsrConfig,
    sherpa: &crate::config::SherpaConfig,
    start_paused: bool,
    backend: AsrBackend,
) -> Result<(Arc<dyn AsrEngine>, mpsc::Receiver<AsrEvent>)> {
    match backend {
        AsrBackend::Bailian => {
            let (client, rx) = BailianAsr::connect(dashscope, asr_cfg, start_paused).await?;
            Ok((Arc::new(client) as Arc<dyn AsrEngine>, rx))
        }
        #[cfg(feature = "sherpa-asr-tts")]
        AsrBackend::Sherpa => {
            let sherpa_cfg = asr_cfg.effective_sherpa(sherpa);
            let (client, rx) =
                SherpaAsr::connect(&sherpa_cfg, asr_cfg.sample_rate, start_paused).await?;
            Ok((Arc::new(client) as Arc<dyn AsrEngine>, rx))
        }
    }
}
