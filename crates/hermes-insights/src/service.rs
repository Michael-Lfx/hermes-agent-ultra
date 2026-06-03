//! Contribution pipeline facade: enqueue, preview, flush.



use std::path::{Path, PathBuf};



use hermes_config::InsightsContributionConfig;

use tracing::{debug, warn};



use crate::client::{ContributionClient, FlushResult};

use crate::interest::{

    build_interest_fingerprint, top_readable_interest_labels, InterestTopicInput,

};

use crate::maturity::{skill_key_from_dir, SkillMaturityStore};

use crate::outbox::ContributionOutbox;

use crate::paths::{audit_path, ensure_state_dir, outbox_path};

use crate::skill::{
    build_skill_pattern, find_skill_dir_by_slug, skill_pattern_options_from_payload,
    walk_unique_skill_dirs, walk_unique_skill_patterns, SkillPatternOptions,
};

use crate::types::{
    dedupe_batch_contributions, envelope_from_value, ContributionBatch, ContributionEnvelope,
    ContributionType, INSIGHTS_CONSENT_VERSION,
};



pub struct ContributionService {

    hermes_home: PathBuf,

    config: InsightsContributionConfig,

    outbox: ContributionOutbox,

}



impl ContributionService {

    pub fn open(hermes_home: PathBuf, config: InsightsContributionConfig) -> Result<Self, String> {

        ensure_state_dir(&hermes_home).map_err(|e| e.to_string())?;

        let outbox = ContributionOutbox::open(&outbox_path(&hermes_home))?;

        Ok(Self {

            hermes_home,

            config,

            outbox,

        })

    }



    pub fn outbox_counts(&self) -> Result<crate::outbox::OutboxCounts, String> {

        self.outbox.counts()

    }

    pub fn reset_outbox(&self, clear_all: bool) -> Result<u32, String> {
        if clear_all {
            self.outbox.clear_all()
        } else {
            self.outbox.reset_sent_to_pending()
        }
    }



    pub fn config(&self) -> &InsightsContributionConfig {

        &self.config

    }



    fn audit_drop(&self, reason: &str, detail: &str) {

        let path = audit_path(&self.hermes_home);

        let line = serde_json::json!({

            "ts": chrono::Utc::now().to_rfc3339(),

            "event": "dropped",

            "reason": reason,

            "detail": detail,

        });

        if let Ok(mut file) = std::fs::OpenOptions::new()

            .create(true)

            .append(true)

            .open(path)

        {

            use std::io::Write;

            let _ = writeln!(file, "{line}");

        }

    }



    fn try_enqueue(&self, envelope: ContributionEnvelope) {

        match self.outbox.enqueue(envelope) {

            Ok(true) => debug!("insights: enqueued contribution"),

            Ok(false) => debug!("insights: duplicate content_hash skipped"),

            Err(e) => warn!("insights: outbox enqueue failed: {e}"),

        }

    }



    pub fn enqueue_interest_snapshot(&self, topics: &[InterestTopicInput]) {

        if !self.config.enabled || !self.config.upload_interests {

            return;

        }

        let Some(fp) = build_interest_fingerprint(topics) else {

            return;

        };

        let collected_at = fp.collected_at.clone();

        let envelope = match envelope_from_value(

            ContributionType::InterestSnapshot,

            &collected_at,

            &fp,

        ) {

            Ok(e) => e,

            Err(e) => {

                self.audit_drop("serialize_error", &e);

                return;

            }

        };

        self.try_enqueue(envelope);

    }



    pub fn enqueue_skill_from_dir(

        &self,

        skill_dir: &Path,

        kind: crate::skill::SkillChangeKind,

        linked_interest_labels: &[String],

        from_background_review: bool,

    ) {

        if !self.config.enabled || !self.config.upload_skills {

            return;

        }

        let skills_root = self.hermes_home.join("skills");

        let skill_md = skill_dir.join("SKILL.md");

        let content = match std::fs::read_to_string(&skill_md) {

            Ok(c) => c,

            Err(_) => return,

        };

        let content_hash = crate::types::sha256_hex(content.as_bytes());

        let skill_key = skill_key_from_dir(skill_dir);

        let mut maturity = match SkillMaturityStore::open(&self.hermes_home) {

            Ok(m) => m,

            Err(e) => {

                warn!("insights maturity store: {e}");

                return;

            }

        };

        maturity.touch_skill(&skill_key, &content_hash);

        let _ = maturity.save();

        if !maturity.is_eligible(

            &skill_key,

            self.config.skill_min_age_hours,

            &content_hash,

        ) {

            debug!(skill = %skill_key, "insights: skill not mature enough");

            return;

        }

        let mut opts = SkillPatternOptions::from_change_kind(kind);

        opts.include_body = self.config.redacted_body;

        opts.linked_interest_labels = linked_interest_labels.to_vec();

        opts.from_background_review = from_background_review;

        let Some(pattern) = build_skill_pattern(skill_dir, &skills_root, &opts) else {

            self.audit_drop("skill_sanitize_failed", &skill_key);

            return;

        };

        let collected_at = chrono::Utc::now().to_rfc3339();

        let envelope = match envelope_from_value(

            ContributionType::SkillPattern,

            &collected_at,

            &pattern,

        ) {

            Ok(e) => e,

            Err(e) => {

                self.audit_drop("serialize_error", &e);

                return;

            }

        };

        self.try_enqueue(envelope);

    }



    pub fn mark_skills_review_patched(&self) {

        if let Ok(mut store) = SkillMaturityStore::open(&self.hermes_home) {

            store.mark_review_patched_all();

            let _ = store.save();

        }

    }



    pub fn preview_interest(&self, topics: &[InterestTopicInput]) -> Option<ContributionEnvelope> {

        let fp = build_interest_fingerprint(topics)?;

        envelope_from_value(

            ContributionType::InterestSnapshot,

            &fp.collected_at,

            &fp,

        )

        .ok()

    }



    pub fn preview_skills(&self, topics: &[InterestTopicInput]) -> Vec<ContributionEnvelope> {

        let linked = top_readable_interest_labels(topics, 5);

        let skills_root = self.hermes_home.join("skills");

        if !skills_root.is_dir() {

            return Vec::new();

        }

        let opts_base = SkillPatternOptions {

            include_body: self.config.redacted_body,

            from_background_review: false,

            linked_interest_labels: linked,

            provenance: crate::types::SkillProvenance::AgentCreated,

        };

        walk_unique_skill_patterns(&skills_root, |skill_dir| {

            build_skill_pattern(skill_dir, &skills_root, &opts_base)

        })

        .into_iter()

        .filter_map(|pattern| {

            envelope_from_value(

                ContributionType::SkillPattern,

                &chrono::Utc::now().to_rfc3339(),

                &pattern,

            )

            .ok()

        })

        .collect()

    }



    pub fn preview_batch(&self, topics: &[InterestTopicInput]) -> ContributionBatch {

        let mut contributions = Vec::new();

        if self.config.upload_interests {

            if let Some(env) = self.preview_interest(topics) {

                contributions.push(env);

            }

        }

        if self.config.upload_skills {

            contributions.extend(self.preview_skills(topics));

        }

        contributions = dedupe_batch_contributions(contributions);

        ContributionBatch {

            batch_id: uuid::Uuid::new_v4().to_string(),

            consent_version: INSIGHTS_CONSENT_VERSION.to_string(),

            contributions,

        }

    }



    /// Re-enqueue all local skills from disk (same path as `preview`), replacing stale outbox rows.
    pub fn sync_skills_from_disk(&self, topics: &[InterestTopicInput]) -> Result<u32, String> {
        if !self.config.enabled || !self.config.upload_skills {
            return Ok(0);
        }
        let envelopes = self.preview_skills(topics);
        let mut synced = 0;
        for env in envelopes {
            match self.outbox.enqueue(env) {
                Ok(true) => synced += 1,
                Ok(false) => debug!("insights sync skill: duplicate content_hash skipped"),
                Err(e) => warn!("insights sync skill enqueue failed: {e}"),
            }
        }
        Ok(synced)
    }

    fn rebuild_skill_envelope_from_disk(
        &self,
        envelope: &ContributionEnvelope,
    ) -> Option<ContributionEnvelope> {
        if envelope.kind != ContributionType::SkillPattern.as_str() {
            return None;
        }
        let name_slug = envelope.payload.get("name_slug")?.as_str()?;
        let skills_root = self.hermes_home.join("skills");
        if !skills_root.is_dir() {
            return None;
        }
        let skill_dir = find_skill_dir_by_slug(&skills_root, name_slug)?;
        let opts = skill_pattern_options_from_payload(&envelope.payload, self.config.redacted_body);
        let pattern = build_skill_pattern(&skill_dir, &skills_root, &opts)?;
        envelope_from_value(
            ContributionType::SkillPattern,
            &envelope.collected_at,
            &pattern,
        )
        .ok()
    }

    fn prepare_skill_upload_envelopes(
        &self,
        limit: usize,
    ) -> Result<(Vec<String>, Vec<ContributionEnvelope>), String> {
        let pending = self.outbox.list_pending(limit)?;
        let mut ids = Vec::with_capacity(pending.len());
        let mut envelopes = Vec::with_capacity(pending.len());
        for entry in pending {
            ids.push(entry.id.clone());
            let envelope = self
                .rebuild_skill_envelope_from_disk(&entry.envelope)
                .unwrap_or(entry.envelope);
            if entry.kind == ContributionType::SkillPattern.as_str() {
                let _ = self.outbox.update_envelope(&entry.id, envelope.clone());
            }
            envelopes.push(envelope);
        }
        Ok((ids, envelopes))
    }

    /// Rebuild pending skill_pattern rows from disk so uploads include fresh fields
    /// (e.g. `references_redacted`) even when outbox still holds older cached JSON.
    pub fn refresh_pending_skill_patterns(&self) -> Result<u32, String> {
        if !self.config.upload_skills {
            return Ok(0);
        }
        let skills_root = self.hermes_home.join("skills");
        if !skills_root.is_dir() {
            return Ok(0);
        }
        let pending = self.outbox.list_pending(512)?;
        let mut updated = 0;
        for entry in pending {
            if entry.kind != ContributionType::SkillPattern.as_str() {
                continue;
            }
            let Some(new_envelope) = self.rebuild_skill_envelope_from_disk(&entry.envelope) else {
                continue;
            };
            let refs_count = new_envelope
                .payload
                .get("references_redacted")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            self.outbox
                .update_envelope(&entry.id, new_envelope.clone())?;
            debug!(
                name_slug = new_envelope
                    .payload
                    .get("name_slug")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
                refs_count,
                content_hash = %new_envelope.content_hash,
                "insights: refreshed skill pattern before upload"
            );
            updated += 1;
        }
        Ok(updated)
    }

    pub async fn flush(&self) -> Result<FlushResult, String> {
        self.flush_with_topics(&[]).await
    }

    pub async fn flush_with_topics(
        &self,
        topics: &[InterestTopicInput],
    ) -> Result<FlushResult, String> {
        if self.config.upload_skills {
            match self.sync_skills_from_disk(topics) {
                Ok(n) if n > 0 => debug!(synced = n, "insights: synced skills from disk to outbox"),
                Ok(_) => {}
                Err(e) => warn!("insights sync skills from disk failed: {e}"),
            }
        }
        if let Err(e) = self.refresh_pending_skill_patterns() {
            warn!("insights refresh pending skills failed: {e}");
        }
        let (ids, envelopes) = self.prepare_skill_upload_envelopes(50)?;
        let client = ContributionClient::new(self.config.clone(), self.hermes_home.clone());
        client
            .upload_prepared(&self.outbox, &ids, envelopes)
            .await
            .map_err(|e| e.to_string())
    }



    pub async fn revoke_installation(&self) -> Result<(), String> {

        let client = ContributionClient::new(self.config.clone(), self.hermes_home.clone());

        client.revoke_installation().await.map_err(|e| e.to_string())

    }



    pub fn spawn_skill_enqueue(skill_dir: &Path, kind: crate::skill::SkillChangeKind) {

        let skill_dir = skill_dir.to_path_buf();

        let hermes_home = hermes_config::hermes_home();

        tokio::spawn(async move {

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            let config = hermes_config::load_config(None)

                .unwrap_or_default()

                .insights

                .contribution;

            if !config.enabled || !config.upload_skills {

                return;

            }

            let upload_ready = config.upload_ready();

            let Ok(svc) = ContributionService::open(hermes_home, config) else {

                return;

            };

            svc.enqueue_skill_from_dir(&skill_dir, kind, &[], false);

            if upload_ready {

                let _ = svc.flush().await;

            }

        });

    }



    pub fn spawn_session_end(

        hermes_home: PathBuf,

        config: InsightsContributionConfig,

        topics: Vec<InterestTopicInput>,

    ) {

        if !config.enabled || !config.on_session_end {

            return;

        }

        let linked_labels = top_readable_interest_labels(&topics, 5);

        tokio::spawn(async move {

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let Ok(svc) = ContributionService::open(hermes_home.clone(), config.clone()) else {

                return;

            };

            if config.upload_interests {

                svc.enqueue_interest_snapshot(&topics);

            }

            if config.upload_skills {

                let skills_root = hermes_home.join("skills");

                if skills_root.is_dir() {

                    walk_unique_skill_dirs(&skills_root, |skill_dir| {

                        svc.enqueue_skill_from_dir(

                            skill_dir,

                            crate::skill::SkillChangeKind::Agent,

                            &linked_labels,

                            false,

                        );

                    });

                }

            }

            if config.upload_ready() {

                let _ = svc.flush().await;

            }

        });

    }

}

