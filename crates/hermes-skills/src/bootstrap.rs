//! Single entry point for skills runtime initialization.

use std::sync::Arc;

use hermes_core::traits::SkillProvider;
use tracing::{info, warn};

use crate::distribution::{BundledLayout, BundledSource};
use crate::layered_store::LayeredSkillStore;
use crate::skill::{SkillError, SkillManager};
use crate::sync::{SkillSyncConfig, SkillSyncResult, sync_skills};

/// Result of [`bootstrap`].
pub struct SkillsBootstrapResult {
    pub provider: Arc<dyn SkillProvider>,
    pub bundled_count: usize,
    pub user_count: usize,
    pub source: BundledSource,
    pub sync: Option<SkillSyncResult>,
    pub warnings: Vec<String>,
}

/// Initialize layered skills runtime: resolve bundled layout, optional reconcile, return provider.
pub fn bootstrap(quiet: bool) -> Result<SkillsBootstrapResult, SkillError> {
    let layout = BundledLayout::resolve();
    let mut warnings = Vec::new();

    if layout.source == BundledSource::Missing {
        warnings.push(
            "bundled skills directory not found; only user-installed skills will be visible"
                .to_string(),
        );
    }

    let store = LayeredSkillStore::new(layout.clone());
    let bundled_count = if layout.bundled_exists() {
        crate::sync::discover_bundled_skills(&layout.bundled_dir).len()
    } else {
        0
    };

    let sync = if layout.bundled_exists() {
        let config = layout.sync_config();
        match reconcile_bundled_updates(&config, quiet) {
            Ok(result) => Some(result),
            Err(err) => {
                warnings.push(format!("bundled skills reconcile skipped: {err}"));
                None
            }
        }
    } else {
        None
    };

    let provider: Arc<dyn SkillProvider> = Arc::new(SkillManager::new(store.into_arc()));
    let user_count = count_user_skills();

    if !quiet {
        if layout.bundled_exists() {
            info!(
                bundled = bundled_count,
                user = user_count,
                source = layout.source.as_str(),
                dir = %layout.bundled_dir.display(),
                "skills runtime initialized"
            );
        } else {
            warn!(
                user = user_count,
                source = layout.source.as_str(),
                "skills runtime initialized without bundled tree"
            );
        }
    }

    Ok(SkillsBootstrapResult {
        provider,
        bundled_count,
        user_count,
        source: layout.source,
        sync,
        warnings,
    })
}

/// Return layered skill search roots (user first, then bundled).
pub fn skill_search_roots() -> Vec<std::path::PathBuf> {
    BundledLayout::resolve().skill_search_roots()
}

/// Reconcile user-layer copies of bundled skills after OTA or manual sync.
pub fn reconcile_bundled_updates(
    config: &SkillSyncConfig,
    quiet: bool,
) -> Result<SkillSyncResult, SkillError> {
    sync_skills(config, quiet)
}

fn count_user_skills() -> usize {
    let skills_dir = hermes_config::skills_dir();
    if !skills_dir.is_dir() {
        return 0;
    }
    walkdir::WalkDir::new(&skills_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name() == "SKILL.md")
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_finds_bundled_in_repo_checkout() {
        let skills_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
        if !skills_dir.is_dir() {
            return;
        }
        std::env::set_current_dir(skills_dir.parent().unwrap()).unwrap();
        let result = bootstrap(true).expect("bootstrap");
        assert!(
            result.bundled_count > 0,
            "expected bundled skills in repo checkout"
        );
    }
}
