//! Layered skill store: user-writable home layer over read-only bundled layer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use hermes_core::types::{Skill, SkillMeta};
use tracing::debug;

use crate::distribution::BundledLayout;
use crate::skill::SkillError;
use crate::store::{FileSkillStore, SkillStore};

/// Merges user-installed skills with bundled read-only skills.
pub struct LayeredSkillStore {
    user: FileSkillStore,
    bundled: Option<FileSkillStore>,
    layout: BundledLayout,
}

impl LayeredSkillStore {
    pub fn new(layout: BundledLayout) -> Self {
        let user = FileSkillStore::new(hermes_config::skills_dir());
        let bundled = layout
            .bundled_exists()
            .then(|| FileSkillStore::new(layout.bundled_dir.clone()));
        Self {
            user,
            bundled,
            layout,
        }
    }

    pub fn layout(&self) -> &BundledLayout {
        &self.layout
    }

    pub fn skill_search_roots(&self) -> Vec<PathBuf> {
        self.layout.skill_search_roots()
    }

    pub fn into_arc(self) -> Arc<dyn SkillStore> {
        Arc::new(self)
    }
}

#[async_trait]
impl SkillStore for LayeredSkillStore {
    async fn save(&self, skill: &Skill) -> Result<(), SkillError> {
        self.user.save(skill).await
    }

    async fn load(&self, name: &str) -> Result<Option<Skill>, SkillError> {
        if let Some(skill) = self.user.load(name).await? {
            return Ok(Some(skill));
        }
        if let Some(bundled) = &self.bundled
            && let Some(skill) = bundled.load(name).await?
        {
            debug!(skill = %name, "loaded skill from bundled layer");
            return Ok(Some(skill));
        }
        Ok(None)
    }

    async fn list(&self) -> Result<Vec<SkillMeta>, SkillError> {
        let mut by_name: HashMap<String, SkillMeta> = HashMap::new();
        if let Some(bundled) = &self.bundled {
            for meta in bundled.list().await? {
                by_name.insert(meta.name.clone(), meta);
            }
        }
        for meta in self.user.list().await? {
            by_name.insert(meta.name.clone(), meta);
        }
        let mut metas: Vec<SkillMeta> = by_name.into_values().collect();
        metas.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(metas)
    }

    async fn delete(&self, name: &str) -> Result<(), SkillError> {
        self.user.delete(name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distribution::BundledSource;
    use std::fs;
    use tempfile::tempdir;

    fn write_skill(base: &std::path::Path, category: &str, name: &str, body: &str) {
        let dir = base.join(category).join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\n---\n{body}\n"),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn bundled_visible_when_user_empty() {
        let root = tempdir().unwrap();
        let bundled_root = root.path().join("bundled");
        write_skill(&bundled_root, "finance", "trading-research", "bundled body");
        let user_root = root.path().join("user");
        fs::create_dir_all(&user_root).unwrap();

        let layout = BundledLayout {
            bundled_dir: bundled_root,
            optional_dir: root.path().join("optional"),
            source: BundledSource::DevRepo,
        };
        // ponytail: test uses explicit paths; production uses hermes_config::skills_dir()
        let store = LayeredSkillStore {
            user: FileSkillStore::new(user_root),
            bundled: Some(FileSkillStore::new(layout.bundled_dir.clone())),
            layout,
        };
        let metas = store.list().await.unwrap();
        assert!(metas.iter().any(|m| m.name == "trading-research"));
        let skill = store.load("trading-research").await.unwrap().unwrap();
        assert!(skill.content.contains("bundled body"));
    }

    #[tokio::test]
    async fn user_layer_overrides_bundled_name() {
        let root = tempdir().unwrap();
        let bundled_root = root.path().join("bundled");
        write_skill(&bundled_root, "finance", "trading-research", "bundled");
        let user_root = root.path().join("user");
        write_skill(&user_root, "finance", "trading-research", "user copy");

        let layout = BundledLayout {
            bundled_dir: bundled_root,
            optional_dir: root.path().join("optional"),
            source: BundledSource::DevRepo,
        };
        let store = LayeredSkillStore {
            user: FileSkillStore::new(user_root),
            bundled: Some(FileSkillStore::new(layout.bundled_dir.clone())),
            layout,
        };
        let skill = store.load("trading-research").await.unwrap().unwrap();
        assert!(skill.content.contains("user copy"));
    }
}
