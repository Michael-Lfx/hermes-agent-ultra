//! Kokoro multi-lang v1.1 speaker names (103 voices).
//!
//! Mapping from https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/kokoro-multi-lang-v1_1.html
//! (model bundled by `scripts/talk/download_models.*`).

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::error::{DemoError, Result};

static KOKORO_MULTI_LANG_V1_1: LazyLock<HashMap<&'static str, i32>> = LazyLock::new(|| {
    HashMap::from([
        ("af_maple", 0),
        ("af_sol", 1),
        ("bf_vale", 2),
        ("zf_001", 3),
        ("zf_002", 4),
        ("zf_003", 5),
        ("zf_004", 6),
        ("zf_005", 7),
        ("zf_006", 8),
        ("zf_007", 9),
        ("zf_008", 10),
        ("zf_017", 11),
        ("zf_018", 12),
        ("zf_019", 13),
        ("zf_021", 14),
        ("zf_022", 15),
        ("zf_023", 16),
        ("zf_024", 17),
        ("zf_026", 18),
        ("zf_027", 19),
        ("zf_028", 20),
        ("zf_032", 21),
        ("zf_036", 22),
        ("zf_038", 23),
        ("zf_039", 24),
        ("zf_040", 25),
        ("zf_042", 26),
        ("zf_043", 27),
        ("zf_044", 28),
        ("zf_046", 29),
        ("zf_047", 30),
        ("zf_048", 31),
        ("zf_049", 32),
        ("zf_051", 33),
        ("zf_059", 34),
        ("zf_060", 35),
        ("zf_067", 36),
        ("zf_070", 37),
        ("zf_071", 38),
        ("zf_072", 39),
        ("zf_073", 40),
        ("zf_074", 41),
        ("zf_075", 42),
        ("zf_076", 43),
        ("zf_077", 44),
        ("zf_078", 45),
        ("zf_079", 46),
        ("zf_083", 47),
        ("zf_084", 48),
        ("zf_085", 49),
        ("zf_086", 50),
        ("zf_087", 51),
        ("zf_088", 52),
        ("zf_090", 53),
        ("zf_092", 54),
        ("zf_093", 55),
        ("zf_094", 56),
        ("zf_099", 57),
        ("zm_009", 58),
        ("zm_010", 59),
        ("zm_011", 60),
        ("zm_012", 61),
        ("zm_013", 62),
        ("zm_014", 63),
        ("zm_015", 64),
        ("zm_016", 65),
        ("zm_020", 66),
        ("zm_025", 67),
        ("zm_029", 68),
        ("zm_030", 69),
        ("zm_031", 70),
        ("zm_033", 71),
        ("zm_034", 72),
        ("zm_035", 73),
        ("zm_037", 74),
        ("zm_041", 75),
        ("zm_045", 76),
        ("zm_050", 77),
        ("zm_052", 78),
        ("zm_053", 79),
        ("zm_054", 80),
        ("zm_055", 81),
        ("zm_056", 82),
        ("zm_057", 83),
        ("zm_058", 84),
        ("zm_061", 85),
        ("zm_062", 86),
        ("zm_063", 87),
        ("zm_064", 88),
        ("zm_065", 89),
        ("zm_066", 90),
        ("zm_068", 91),
        ("zm_069", 92),
        ("zm_080", 93),
        ("zm_081", 94),
        ("zm_082", 95),
        ("zm_089", 96),
        ("zm_091", 97),
        ("zm_095", 98),
        ("zm_096", 99),
        ("zm_097", 100),
        ("zm_098", 101),
        ("zm_100", 102),
    ])
});

/// Resolve Kokoro speaker: `voice` name or numeric string overrides `sid`.
pub fn resolve_kokoro_sid(voice: Option<&str>, sid: i32) -> Result<i32> {
    let Some(raw) = voice.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(sid);
    };
    if let Ok(n) = raw.parse::<i32>() {
        if (0..=102).contains(&n) {
            return Ok(n);
        }
        return Err(DemoError::Config(format!(
            "tts.sherpa.kokoro.voice sid '{n}' out of range (0-102 for kokoro-multi-lang-v1_1)"
        )));
    }
    let key = raw.to_ascii_lowercase();
    KOKORO_MULTI_LANG_V1_1
        .get(key.as_str())
        .copied()
        .ok_or_else(|| {
            DemoError::Config(format!(
                "unknown Kokoro voice '{raw}' (see kokoro-multi-lang-v1_1 speaker list in sherpa docs)"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_voice_name_and_numeric_sid() {
        assert_eq!(resolve_kokoro_sid(Some("zf_048"), 0).unwrap(), 31);
        assert_eq!(resolve_kokoro_sid(Some("31"), 0).unwrap(), 31);
        assert_eq!(resolve_kokoro_sid(None, 3).unwrap(), 3);
    }

    #[test]
    fn rejects_unknown_voice() {
        assert!(resolve_kokoro_sid(Some("not_a_voice"), 0).is_err());
        assert!(resolve_kokoro_sid(Some("zf_xiaoyi"), 0).is_err());
    }
}
