//! Hermes Skills Crate
//!
//! Implements the skills system (Requirement 12) for Hermes Agent.
//! Provides skill management, local file storage, hub client, security
//! validation, and versioning.

mod curator;
mod curator_prompt;
mod guard;
mod hub;
mod hub_lock;
mod provenance;
mod skill;
mod skills_guard;
mod store;
mod sync;
mod usage;
mod version;

pub use curator::{
    AbsorbedDeclaration, ClassificationResult, ConsolidationEntry, CuratorConfig, CuratorError,
    CuratorReviewResult, CuratorRunCounts, CuratorRunRecord, CuratorRunReport, CuratorState,
    PruningEntry, StructuredSummary, ToolCallRecord, TransitionResult, apply_automatic_transitions,
    build_curator_prompt, classify_removed_skills, extract_absorbed_into_declarations, is_paused,
    load_curator_state, maybe_run_curator, parse_structured_summary, reconcile_classification,
    run_curator_review, save_curator_state, set_paused, should_run_now, write_curator_report,
};
pub use curator_prompt::CURATOR_REVIEW_PROMPT;
pub use guard::{
    MAX_SINGLE_SKILL_FILE_BYTES, MAX_SKILL_FILE_COUNT, SkillGuard, SkillScanFinding,
    SkillScanReport, SkillScanVerdict, SkillTrustLevel, check_skill_structure,
    content_hash as guard_content_hash, determine_verdict as guard_determine_verdict,
    resolve_trust_level as guard_resolve_trust_level, scan_skill_dir, scan_skill_file,
    should_allow_install as guard_should_allow_install, validate_skill, validate_skill_url,
};
pub use hub::{
    ClawHubBundle, ClawHubFileRef, RegistrySkillMeta, SkillUpdate, SkillsHubClient,
    clawhub_file_refs, clawhub_finalize_search_results, clawhub_latest_version,
    clawhub_meta_from_payload, clawhub_metas_from_listing,
};
pub use hub_lock::{
    HUB_LOCK_FILE, HUB_LOCK_VERSION, HUB_STATE_DIR, SkillHubInstalledEntry, SkillsHubLock,
    hub_lock_path, read_hub_lock, resolve_scan_source,
};
pub use provenance::{
    ASSISTANT_TOOL, BACKGROUND_REVIEW, FOREGROUND, WriteOriginGuard, get_current_write_origin,
    is_background_review, normalize_write_origin, set_current_write_origin,
};
pub use skill::{MAX_SKILL_CONTENT_CHARS, SkillError, SkillManager};
pub use skills_guard::{
    Finding, InstallDecision, ScanResult, TRUSTED_REPOS, content_hash, determine_verdict,
    resolve_trust_level, scan_bundle, scan_content, scan_skill, should_allow_install,
};
pub use store::{FileSkillStore, MAX_SKILL_NAME_LENGTH, SkillStore};
pub use sync::{
    BundledSkill, BundledSkillsOptOutResult, NO_BUNDLED_SKILLS_MARKER,
    OfficialOptionalRestoreResult, PristineBundledSkillSkip, RemovePristineBundledSkillsResult,
    SkillResetResult, SkillSyncConfig, SkillSyncResult, bundled_skills_opt_out_marker,
    compute_relative_dest, dir_hash, discover_bundled_skills, is_bundled_skills_opt_out,
    read_manifest, read_skill_name, remove_pristine_bundled_skills, reset_bundled_skill,
    restore_official_optional_skill, set_bundled_skills_opt_out, sync_skills, write_manifest,
};
pub use usage::{
    STATE_ACTIVE, STATE_ARCHIVED, STATE_STALE, SkillUsageRecord, SkillUsageReportRow, UsageStore,
    is_agent_created, is_protected_skill,
};
pub use version::{SkillChange, SkillVersion, compare_versions, compute_version, track_change};
