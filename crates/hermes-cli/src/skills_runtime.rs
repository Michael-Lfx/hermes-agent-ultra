//! Unified skills runtime bootstrap for CLI and gateway entry points.

use std::sync::Arc;

use hermes_core::traits::SkillProvider;
use hermes_skills::{SkillError, bootstrap};

/// Build a [`SkillProvider`] with layered bundled + user skills.
pub fn build_skill_provider(quiet: bool) -> Result<SkillsRuntime, SkillError> {
    let result = bootstrap(quiet)?;
    Ok(SkillsRuntime {
        provider: result.provider,
        bundled_count: result.bundled_count,
        user_count: result.user_count,
        source: result.source,
        warnings: result.warnings,
    })
}

/// Initialized skills runtime handle.
pub struct SkillsRuntime {
    pub provider: Arc<dyn SkillProvider>,
    pub bundled_count: usize,
    pub user_count: usize,
    pub source: hermes_skills::BundledSource,
    pub warnings: Vec<String>,
}

impl SkillsRuntime {
    /// Skill filesystem roots for `skill_view` (user layer first).
    pub fn skill_search_roots(&self) -> Vec<std::path::PathBuf> {
        hermes_skills::skill_search_roots()
    }
}
