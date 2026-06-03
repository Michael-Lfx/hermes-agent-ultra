use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 发布清单 — 从服务端获取的版本元数据
///
/// 兼容两种格式：
/// - 新格式：含 platforms/channel/forced/min_version/pub_date
/// - 旧格式：仅含 version + artifacts 数组
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    /// 版本号（必填，如 "1.2.0" 或 "1.0.0-beta.1"）
    pub version: String,

    /// 发布渠道：stable/beta/rc/nightly（默认 stable）
    #[serde(default = "default_stable")]
    pub channel: String,

    /// 发布时间（RFC 3339 格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pub_date: Option<String>,

    /// 是否强制更新
    #[serde(default)]
    pub forced: bool,

    /// 最低兼容版本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,

    /// 发布说明
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// 按平台的下载信息（新格式）
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub platforms: HashMap<String, PlatformArtifact>,

    /// artifact 文件名列表（旧格式兼容字段）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,

    /// 旧格式的 tag 字段（兼容）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// 平台 artifact 详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformArtifact {
    /// 下载 URL
    pub url: String,

    /// SHA256 哈希
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,

    /// 文件大小（字节）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

fn default_stable() -> String {
    "stable".to_string()
}

impl ReleaseManifest {
    /// 获取指定平台的 artifact 信息
    /// platform_key 格式: "linux-x86_64", "windows-x86_64", "macos-aarch64"
    pub fn get_platform(&self, platform_key: &str) -> Option<&PlatformArtifact> {
        self.platforms.get(platform_key)
    }

    /// 获取版本的 tag 形式（带 v 前缀）
    pub fn version_tag(&self) -> String {
        if let Some(ref tag) = self.tag {
            tag.clone()
        } else {
            format!("v{}", self.version)
        }
    }

    /// 判断是否为新格式（含 platforms 字段）
    pub fn has_platform_info(&self) -> bool {
        !self.platforms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_new_format() {
        let json = r#"{
            "version": "1.2.0",
            "channel": "stable",
            "pub_date": "2026-06-03T12:00:00Z",
            "forced": false,
            "min_version": "0.10.0",
            "notes": "Bug fixes",
            "platforms": {
                "linux-x86_64": {
                    "url": "https://example.com/hermes-linux-x86_64.tar.gz",
                    "sha256": "abcd1234",
                    "size": 12345678
                },
                "windows-x86_64": {
                    "url": "https://example.com/hermes-windows-x86_64.zip",
                    "sha256": "efgh5678",
                    "size": 9876543
                }
            }
        }"#;

        let manifest: ReleaseManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "1.2.0");
        assert_eq!(manifest.channel, "stable");
        assert_eq!(manifest.forced, false);
        assert_eq!(manifest.min_version, Some("0.10.0".to_string()));
        assert!(manifest.has_platform_info());

        let linux = manifest.get_platform("linux-x86_64").unwrap();
        assert_eq!(linux.sha256, Some("abcd1234".to_string()));
        assert_eq!(linux.size, Some(12345678));
    }

    #[test]
    fn test_parse_old_format() {
        let json = r#"{
            "version": "0.1.0",
            "tag": "v0.1.0",
            "artifacts": ["hermes-linux-x86_64.tar.gz", "hermes-windows-x86_64.zip"]
        }"#;

        let manifest: ReleaseManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.channel, "stable"); // default
        assert_eq!(manifest.forced, false); // default
        assert_eq!(manifest.artifacts.len(), 2);
        assert!(!manifest.has_platform_info());
        assert_eq!(manifest.version_tag(), "v0.1.0");
    }

    #[test]
    fn test_parse_minimal() {
        let json = r#"{"version": "2.0.0"}"#;
        let manifest: ReleaseManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(manifest.channel, "stable");
        assert_eq!(manifest.forced, false);
        assert!(manifest.platforms.is_empty());
        assert!(manifest.artifacts.is_empty());
    }

    #[test]
    fn test_parse_beta_channel() {
        let json = r#"{
            "version": "1.0.0-beta.1",
            "channel": "beta",
            "forced": true,
            "platforms": {}
        }"#;
        let manifest: ReleaseManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.channel, "beta");
        assert_eq!(manifest.forced, true);
    }

    #[test]
    fn test_version_tag_default() {
        let json = r#"{"version": "1.5.0"}"#;
        let manifest: ReleaseManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version_tag(), "v1.5.0");
    }

    #[test]
    fn test_get_platform_missing() {
        let json = r#"{"version": "1.0.0", "platforms": {"linux-x86_64": {"url": "http://x"}}}"#;
        let manifest: ReleaseManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.get_platform("linux-x86_64").is_some());
        assert!(manifest.get_platform("windows-x86_64").is_none());
    }
}
