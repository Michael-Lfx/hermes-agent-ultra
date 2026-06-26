mod bailian;
#[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
mod kokoro_server;
#[cfg(all(
    feature = "rockchip",
    target_arch = "aarch64",
    not(feature = "sherpa-asr-tts")
))]
pub mod rk_tts;
#[cfg(feature = "sherpa-asr-tts")]
mod sherpa_tts;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::backends::{TalkBackendKind, classify_talk_backend};
use crate::config::{DashscopeConfig, TtsConfig};
use crate::error::Result;

pub use bailian::BailianTts;
pub use bailian::TtsAudio;

#[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
pub use kokoro_server::KokoroServerTts;
#[cfg(all(
    feature = "rockchip",
    target_arch = "aarch64",
    not(all(feature = "rockchip", feature = "sherpa-asr-tts"))
))]
pub use rk_tts::RockchipTts;
#[cfg(feature = "sherpa-asr-tts")]
pub use sherpa_tts::SherpaTts;

#[async_trait]
pub trait TtsEngine: Send + Sync {
    async fn warmup(&self) -> Result<()>;
    async fn append_text(&self, text: &str) -> Result<()>;
    async fn finish_turn(&self) -> Result<()>;
    async fn interrupt_turn(&self) -> Result<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum TtsBackend {
    Bailian,
    #[cfg(feature = "sherpa-asr-tts")]
    Sherpa,
    #[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
    KokoroServer,
    #[cfg(all(
        feature = "rockchip",
        target_arch = "aarch64",
        not(all(feature = "rockchip", feature = "sherpa-asr-tts"))
    ))]
    Rockchip,
}

impl TtsBackend {
    pub fn from_config(tts_cfg: &TtsConfig) -> Self {
        match classify_talk_backend(&tts_cfg.backend) {
            TalkBackendKind::Cloud => TtsBackend::Bailian,
            TalkBackendKind::Sherpa | TalkBackendKind::LocalHardware => resolve_local_sherpa_tts(),
        }
    }
}

#[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
fn resolve_local_sherpa_tts() -> TtsBackend {
    TtsBackend::KokoroServer
}

#[cfg(all(
    feature = "sherpa-asr-tts",
    not(all(feature = "rockchip", feature = "sherpa-asr-tts"))
))]
fn resolve_local_sherpa_tts() -> TtsBackend {
    TtsBackend::Sherpa
}

#[cfg(all(
    feature = "rockchip",
    target_arch = "aarch64",
    not(feature = "sherpa-asr-tts")
))]
fn resolve_local_sherpa_tts() -> TtsBackend {
    TtsBackend::Rockchip
}

#[cfg(not(any(
    feature = "sherpa-asr-tts",
    all(feature = "rockchip", target_arch = "aarch64")
)))]
fn resolve_local_sherpa_tts() -> TtsBackend {
    TtsBackend::Bailian
}

pub async fn create_tts(
    dashscope: &DashscopeConfig,
    tts_cfg: &TtsConfig,
    backend: TtsBackend,
) -> Result<(Arc<dyn TtsEngine>, mpsc::Receiver<TtsAudio>)> {
    match backend {
        TtsBackend::Bailian => {
            let (client, rx) = BailianTts::connect(dashscope, tts_cfg).await?;
            Ok((Arc::new(client) as Arc<dyn TtsEngine>, rx))
        }
        #[cfg(feature = "sherpa-asr-tts")]
        TtsBackend::Sherpa => {
            let sherpa_cfg = tts_cfg.effective_sherpa();
            let (client, rx) = SherpaTts::connect(&sherpa_cfg).await?;
            Ok((Arc::new(client) as Arc<dyn TtsEngine>, rx))
        }
        #[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
        TtsBackend::KokoroServer => {
            let cfg = tts_cfg.effective_kokoro_server();
            let (client, rx) = KokoroServerTts::connect(&cfg).await?;
            Ok((Arc::new(client) as Arc<dyn TtsEngine>, rx))
        }
        #[cfg(all(
            feature = "rockchip",
            target_arch = "aarch64",
            not(all(feature = "rockchip", feature = "sherpa-asr-tts"))
        ))]
        TtsBackend::Rockchip => {
            let rockchip_cfg = tts_cfg
                .local
                .as_ref()
                .or(tts_cfg.rockchip.as_ref())
                .ok_or_else(|| {
                    crate::error::DemoError::Config(
                        "tts.local config required when backend = \"local\"".into(),
                    )
                })?;
            let (client, rx) = RockchipTts::connect(rockchip_cfg).await?;
            Ok((Arc::new(client) as Arc<dyn TtsEngine>, rx))
        }
    }
}
