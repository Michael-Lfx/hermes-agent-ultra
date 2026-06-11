use crate::alpha_runtime::QuorumPolicy;
use crate::model_switch::provider_model_ids;
use hermes_config::GatewayConfig;

use super::super::App;
use super::super::provider::resolve_provider_and_model;

impl App {
    fn normalize_quorum_model_target(current_model: &str, raw: &str) -> String {
        let candidate = raw.trim();
        if candidate.is_empty() {
            return current_model.trim().to_string();
        }
        if let Some((provider, model)) = candidate.split_once(':') {
            return format!("{}:{}", provider.trim().to_ascii_lowercase(), model.trim());
        }
        let (provider, _) = resolve_provider_and_model(&GatewayConfig::default(), current_model);
        format!("{}:{}", provider.trim().to_ascii_lowercase(), candidate)
    }

    fn split_provider_model(provider_model: &str) -> (&str, &str) {
        if let Some((provider, model)) = provider_model.split_once(':') {
            (provider, model)
        } else {
            ("", provider_model)
        }
    }

    fn looks_like_version_pinned_model(model_id: &str) -> bool {
        let tail = model_id
            .trim()
            .rsplit('/')
            .next()
            .unwrap_or(model_id)
            .to_ascii_lowercase();
        tail.as_bytes()
            .windows(8)
            .any(|window| window.iter().all(|byte| byte.is_ascii_digit()))
    }

    pub(in crate::app) fn resolve_quorum_catalog_candidate(
        requested_model: &str,
        catalog: &[String],
    ) -> Option<String> {
        if catalog.is_empty() {
            return None;
        }
        let requested_trimmed = requested_model.trim();
        if requested_trimmed.is_empty() {
            return catalog.first().cloned();
        }
        if let Some(hit) = catalog
            .iter()
            .find(|m| m.trim().eq_ignore_ascii_case(requested_trimmed))
        {
            return Some(hit.clone());
        }
        let requested_lc = requested_trimmed.to_ascii_lowercase();
        let slash_suffix = format!("/{}", requested_lc);
        if let Some(hit) = catalog.iter().find(|m| {
            let lower = m.trim().to_ascii_lowercase();
            lower.ends_with(&slash_suffix) || lower == requested_lc
        }) {
            return Some(hit.clone());
        }
        if Self::looks_like_version_pinned_model(requested_trimmed) {
            return None;
        }
        Self::rank_catalog_candidates(requested_trimmed, catalog, 1)
            .into_iter()
            .next()
    }

    pub(in crate::app) fn rank_catalog_candidates(
        requested_model: &str,
        catalog: &[String],
        limit: usize,
    ) -> Vec<String> {
        if catalog.is_empty() || limit == 0 {
            return Vec::new();
        }
        let requested = requested_model.trim().to_ascii_lowercase();
        if requested.is_empty() {
            return catalog.iter().take(limit).cloned().collect();
        }
        let requested_tail = requested.rsplit('/').next().unwrap_or(requested.as_str());
        let requested_norm: String = requested
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();

        let mut scored: Vec<(usize, usize, String)> = catalog
            .iter()
            .enumerate()
            .filter_map(|(idx, candidate)| {
                let cand_trimmed = candidate.trim();
                if cand_trimmed.is_empty() {
                    return None;
                }
                let cand = cand_trimmed.to_ascii_lowercase();
                let cand_tail = cand.rsplit('/').next().unwrap_or(cand.as_str());
                let cand_norm: String =
                    cand.chars().filter(|c| c.is_ascii_alphanumeric()).collect();

                let mut score = 0usize;
                if cand == requested {
                    score += 10_000;
                }
                if cand_tail == requested_tail {
                    score += 8_000;
                }
                if cand.ends_with(&format!("/{}", requested_tail)) {
                    score += 6_000;
                }
                if cand.contains(requested_tail) || requested_tail.contains(cand_tail) {
                    score += 2_000;
                }

                let shared_prefix = requested_norm
                    .chars()
                    .zip(cand_norm.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                score += shared_prefix.saturating_mul(40);

                let shared_chars = requested_norm
                    .chars()
                    .filter(|ch| cand_norm.contains(*ch))
                    .count();
                score += shared_chars.saturating_mul(12);

                let len_delta = requested_norm.len().abs_diff(cand_norm.len());
                score = score.saturating_sub(len_delta.saturating_mul(4));
                if score == 0 {
                    return None;
                }
                Some((score, idx, cand_trimmed.to_string()))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, candidate)| candidate)
            .collect()
    }

    pub(super) async fn resolve_quorum_models(
        &self,
        policy: &QuorumPolicy,
    ) -> (Vec<String>, Vec<String>) {
        let raw = Self::collect_quorum_models(policy, &self.model.current_model);
        if raw.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let mut notes = Vec::new();
        let mut resolved = Vec::new();
        for raw_target in raw {
            let normalized =
                Self::normalize_quorum_model_target(&self.model.current_model, &raw_target);
            let (provider, model_id) = Self::split_provider_model(&normalized);
            let provider = provider.trim().to_ascii_lowercase();
            let model_id = model_id.trim();
            if provider.is_empty() || model_id.is_empty() {
                continue;
            }
            let mut final_target = normalized.clone();
            let catalog = provider_model_ids(&provider).await;
            if !catalog.is_empty() {
                if let Some(candidate) = Self::resolve_quorum_catalog_candidate(model_id, &catalog)
                {
                    final_target = format!("{}:{}", provider, candidate.trim());
                    if !final_target.eq_ignore_ascii_case(&normalized) {
                        notes.push(format!(
                            "quorum model remapped via catalog: {} -> {}",
                            normalized, final_target
                        ));
                    }
                } else if Self::looks_like_version_pinned_model(model_id) {
                    notes.push(format!(
                        "quorum model preserved despite catalog miss: {}",
                        normalized
                    ));
                } else if let Some(fallback) = catalog.first() {
                    let ranked = Self::rank_catalog_candidates(model_id, &catalog, 3);
                    final_target = format!("{}:{}", provider, fallback.trim());
                    notes.push(format!(
                        "quorum model not in provider catalog: {} ; fallback -> {} ; close matches: {}",
                        normalized,
                        final_target,
                        if ranked.is_empty() {
                            "(none)".to_string()
                        } else {
                            ranked.join(", ")
                        }
                    ));
                }
            }
            if !resolved
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&final_target))
            {
                resolved.push(final_target);
            }
        }
        (resolved, notes)
    }
}
