//! De-identified contribution of local POI fingerprints and skill patterns
//! to an external ops server via REST.

pub mod client;
pub mod response;
pub mod interest;
pub use interest::{build_interest_fingerprint, InterestTopicInput};
pub mod maturity;
pub mod outbox;
pub mod paths;
pub mod sanitize;
pub mod service;
pub mod skill;
pub mod types;

pub use client::{ContributionClient, FlushResult};
pub use paths::{
    audit_path, installation_id_path, last_batch_path, load_or_create_installation_id,
    outbox_path, skill_state_path, state_dir,
};
pub use service::ContributionService;
pub use skill::SkillChangeKind;
pub use types::{
    ContributionBatch, ContributionEnvelope, ContributionType, InterestFingerprint,
    InterestTopicFingerprint, SkillPattern, SkillStructure, SkillTriggerHints,
    INSIGHTS_CONSENT_VERSION,
};

/// Fire-and-forget notification after a local skill file changes.
pub fn notify_skill_changed(skill_dir: &std::path::Path, kind: SkillChangeKind) {
    ContributionService::spawn_skill_enqueue(skill_dir, kind);
}
