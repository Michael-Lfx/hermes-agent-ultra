//! Shared sherpa-onnx runtime settings (ONNX Runtime / RKNN execution provider).

use crate::error::{DemoError, Result};

#[cfg(not(all(feature = "rockchip", feature = "sherpa-asr-tts")))]
pub const PLATFORM_PROVIDERS: &[&str] = &["cpu"];

#[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
pub const PLATFORM_PROVIDERS: &[&str] = &["cpu", "rknn"];

pub fn platform_supports(provider: &str) -> bool {
    PLATFORM_PROVIDERS.contains(&provider)
}

pub fn validate_provider(provider: &str) -> Result<()> {
    if platform_supports(provider) {
        Ok(())
    } else {
        #[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
        let hint = "expected 'cpu' or 'rknn'";
        #[cfg(not(all(feature = "rockchip", feature = "sherpa-asr-tts")))]
        let hint = "only 'cpu' is supported";
        Err(DemoError::Config(format!(
            "invalid sherpa provider '{provider}' ({hint})"
        )))
    }
}

/// SenseVoice RKNN models require provider `rknn` on Rockchip builds.
#[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
pub fn infer_asr_provider(provider: &str, model: &str) -> String {
    let p = provider.trim();
    if p != "cpu" && !p.is_empty() {
        return p.to_string();
    }
    if model.to_ascii_lowercase().ends_with(".rknn") {
        return "rknn".to_string();
    }
    p.to_string()
}

#[cfg(not(all(feature = "rockchip", feature = "sherpa-asr-tts")))]
pub fn infer_asr_provider(provider: &str, _model: &str) -> String {
    provider.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_supported() {
        validate_provider("cpu").unwrap();
    }

    #[test]
    fn rejects_directml_and_coreml() {
        assert!(validate_provider("directml").is_err());
        assert!(validate_provider("coreml").is_err());
        assert!(validate_provider("gpu").is_err());
    }

    #[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
    #[test]
    fn rknn_supported_on_rockchip() {
        validate_provider("rknn").unwrap();
    }

    #[cfg(all(feature = "rockchip", feature = "sherpa-asr-tts"))]
    #[test]
    fn infer_asr_provider_from_rknn_model() {
        assert_eq!(
            infer_asr_provider(
                "cpu",
                "models/sensevoice-rk3588/model.rknn"
            ),
            "rknn"
        );
        assert_eq!(
            infer_asr_provider("rknn", "models/sensevoice-rk3588/x.rknn"),
            "rknn"
        );
        assert_eq!(
            infer_asr_provider("cpu", "models/sensevoice/model.onnx"),
            "cpu"
        );
    }
}
