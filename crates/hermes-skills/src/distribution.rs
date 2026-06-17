//! Resolve bundled skill trees on disk (release sidecar, env override, dev repo).

use std::path::{Path, PathBuf};

/// Where bundled skill files were resolved from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundledSource {
    /// Explicit `HERMES_BUNDLED_SKILLS_DIR` (or legacy alias).
    Env,
    /// Next to the executable (`skills/` or `share/hermes/skills/`).
    Sidecar,
    /// Extracted embed cache under hermes home.
    EmbedCache,
    /// Local git checkout (dev / `cargo run` from repo root).
    DevRepo,
    /// No bundled tree found.
    Missing,
}

impl BundledSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Sidecar => "sidecar",
            Self::EmbedCache => "embed_cache",
            Self::DevRepo => "dev_repo",
            Self::Missing => "missing",
        }
    }
}

/// Resolved bundled + optional skill directory layout.
#[derive(Debug, Clone)]
pub struct BundledLayout {
    pub bundled_dir: PathBuf,
    pub optional_dir: PathBuf,
    pub source: BundledSource,
}

impl BundledLayout {
    /// Resolve bundled skill directories for the current process.
    pub fn resolve() -> Self {
        if let Some(layout) = Self::from_env() {
            return layout;
        }
        if let Some(layout) = Self::from_sidecar() {
            return layout;
        }
        if let Some(layout) = Self::from_embed_cache() {
            return layout;
        }
        if let Some(layout) = Self::from_dev_repo() {
            return layout;
        }
        Self {
            bundled_dir: PathBuf::new(),
            optional_dir: PathBuf::new(),
            source: BundledSource::Missing,
        }
    }

    pub fn bundled_exists(&self) -> bool {
        self.source != BundledSource::Missing && skills_tree_has_content(&self.bundled_dir)
    }

    /// Search roots for `skill_view`: user home first, then bundled (if any).
    pub fn skill_search_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![hermes_config::skills_dir()];
        if self.bundled_exists() {
            roots.push(self.bundled_dir.clone());
        }
        if let Some(home) = user_home_dir() {
            let legacy = home.join(hermes_config::LEGACY_HOME_DIR).join("skills");
            if legacy.exists() {
                roots.push(legacy);
            }
        }
        roots.dedup();
        roots
    }

    pub fn sync_config(&self) -> crate::sync::SkillSyncConfig {
        crate::sync::SkillSyncConfig::new(
            self.bundled_dir.clone(),
            self.optional_dir.clone(),
            hermes_config::skills_dir(),
        )
    }

    fn from_env() -> Option<Self> {
        for key in ["HERMES_BUNDLED_SKILLS_DIR", "HERMES_BUNDLED_SKILLS"] {
            if let Ok(raw) = std::env::var(key) {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let bundled = PathBuf::from(trimmed);
                if skills_tree_has_content(&bundled) {
                    let optional = bundled
                        .parent()
                        .map(|p| p.join("optional-skills"))
                        .filter(|p| p.is_dir())
                        .unwrap_or_else(|| bundled.join("../optional-skills"));
                    return Some(Self {
                        bundled_dir: bundled,
                        optional_dir: optional,
                        source: BundledSource::Env,
                    });
                }
            }
        }
        None
    }

    fn from_sidecar() -> Option<Self> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent().map(PathBuf::from);
            while let Some(d) = dir {
                candidates.push(d.join("skills"));
                candidates.push(d.join("share").join("hermes").join("skills"));
                dir = d.parent().map(PathBuf::from);
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join("skills"));
            candidates.push(cwd.join("share").join("hermes").join("skills"));
        }
        for bundled in candidates {
            if skills_tree_has_content(&bundled) {
                let optional = bundled
                    .parent()
                    .and_then(|p| {
                        let a = p.join("optional-skills");
                        let b = p.join("share").join("hermes").join("optional-skills");
                        if a.is_dir() {
                            Some(a)
                        } else if b.is_dir() {
                            Some(b)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| bundled.join("../optional-skills"));
                return Some(Self {
                    bundled_dir: bundled,
                    optional_dir: optional,
                    source: BundledSource::Sidecar,
                });
            }
        }
        None
    }

    fn from_embed_cache() -> Option<Self> {
        let version = env!("CARGO_PKG_VERSION");
        let cache = hermes_config::hermes_home()
            .join(".cache")
            .join("bundled-skills")
            .join(version)
            .join("skills");
        if skills_tree_has_content(&cache) {
            let optional = cache
                .parent()
                .map(|p| p.join("optional-skills"))
                .filter(|p| p.is_dir())
                .unwrap_or_else(|| cache.join("../optional-skills"));
            return Some(Self {
                bundled_dir: cache,
                optional_dir: optional,
                source: BundledSource::EmbedCache,
            });
        }
        None
    }

    fn from_dev_repo() -> Option<Self> {
        if let Ok(root) = std::env::var("HERMES_DEV_SKILLS_ROOT") {
            let bundled = PathBuf::from(root.trim()).join("skills");
            if skills_tree_has_content(&bundled) {
                let optional = bundled
                    .parent()
                    .map(|p| p.join("optional-skills"))
                    .unwrap_or_default();
                return Some(Self {
                    bundled_dir: bundled,
                    optional_dir: optional,
                    source: BundledSource::DevRepo,
                });
            }
        }
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd);
        }
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent().map(PathBuf::from);
            while let Some(d) = dir {
                dirs.push(d.to_path_buf());
                dir = d.parent().map(PathBuf::from);
            }
        }
        for base in dirs {
            let bundled = base.join("skills");
            if skills_tree_has_content(&bundled) {
                let optional = base.join("optional-skills");
                return Some(Self {
                    bundled_dir: bundled,
                    optional_dir: optional,
                    source: BundledSource::DevRepo,
                });
            }
        }
        None
    }
}

fn skills_tree_has_content(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| e.file_type().is_file() && e.file_name() == "SKILL.md")
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("USERPROFILE")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_skill(dir: &Path, category: &str, name: &str) {
        let skill_dir = dir.join(category).join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\n---\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn bundled_exists_when_skill_md_present() {
        let root = tempdir().unwrap();
        write_skill(root.path(), "finance", "demo-skill");
        let layout = BundledLayout {
            bundled_dir: root.path().to_path_buf(),
            optional_dir: root.path().join("optional-skills"),
            source: BundledSource::DevRepo,
        };
        assert!(layout.bundled_exists());
    }
}
