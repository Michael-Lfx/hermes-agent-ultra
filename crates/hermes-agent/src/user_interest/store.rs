//! SQLite-backed local user interest (POI) store.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use hermes_config::InterestConfig;
use rusqlite::{params, Connection};

const ENTRY_DELIMITER: &str = "\n§\n";

/// A single interest topic row.
#[derive(Debug, Clone)]
pub struct InterestTopic {
    pub id: String,
    pub label: String,
    pub summary: String,
    pub weight: f64,
    pub last_seen_at: DateTime<Utc>,
    pub evidence_count: u32,
    pub tags: Vec<String>,
}

/// Incremental update from rules or LLM extraction.
#[derive(Debug, Clone)]
pub struct InterestSignal {
    pub id: String,
    pub label: String,
    pub summary: String,
    pub weight_delta: f64,
    pub tags: Vec<String>,
}

/// Local interest database.
pub struct InterestStore {
    conn: Mutex<Connection>,
    config: InterestConfig,
}

impl InterestStore {
    pub fn open(db_path: &Path, config: InterestConfig) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        let store = Self {
            conn: Mutex::new(conn),
            config,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS topics (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                weight REAL NOT NULL DEFAULT 0.1,
                last_seen_at TEXT NOT NULL,
                evidence_count INTEGER NOT NULL DEFAULT 0,
                tags TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_topics_weight ON topics(weight DESC);
            CREATE INDEX IF NOT EXISTS idx_topics_last_seen ON topics(last_seen_at DESC);",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn config(&self) -> &InterestConfig {
        &self.config
    }

    pub fn apply_decay(&self) -> Result<(), String> {
        let half_life = self.config.decay_half_life_days.max(1.0);
        let factor = 0.5_f64.powf(1.0 / half_life);
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE topics SET weight = MAX(0.05, weight * ?1), updated_at = ?2",
            params![factor, Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn ingest_signals(&self, signals: &[InterestSignal]) -> Result<(), String> {
        let signals: Vec<InterestSignal> =
            super::extract::filter_poi_signals(signals.to_vec());
        if signals.is_empty() {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        for sig in signals {
            let tags_json =
                serde_json::to_string(&sig.tags).unwrap_or_else(|_| "[]".to_string());
            let existing: Option<(f64, u32, String, String)> = conn
                .query_row(
                    "SELECT weight, evidence_count, summary, label FROM topics WHERE id = ?1",
                    params![sig.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .ok();
            if let Some((weight, count, old_summary, old_label)) = existing {
                let new_weight = (weight + sig.weight_delta).min(1.0);
                let summary = if sig.summary.len() > old_summary.len() {
                    sig.summary.clone()
                } else {
                    old_summary
                };
                let label = merge_topic_label(&old_label, &sig.label);
                conn.execute(
                    "UPDATE topics SET label = ?1, summary = ?2, weight = ?3,
                     last_seen_at = ?4, evidence_count = ?5, tags = ?6, updated_at = ?4
                     WHERE id = ?7",
                    params![
                        label,
                        summary,
                        new_weight,
                        now,
                        count + 1,
                        tags_json,
                        sig.id,
                    ],
                )
                .map_err(|e| e.to_string())?;
            } else {
                let weight = sig.weight_delta.clamp(0.08, 0.5);
                conn.execute(
                    "INSERT INTO topics (id, label, summary, weight, last_seen_at, evidence_count, tags, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?5, ?5)",
                    params![sig.id, sig.label, sig.summary, weight, now, tags_json],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        drop(conn);
        self.enforce_max_topics()?;
        Ok(())
    }

    fn enforce_max_topics(&self) -> Result<(), String> {
        let max = self.config.max_topics.max(5) as i64;
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM topics", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if count <= max {
            return Ok(());
        }
        let excess = count - max;
        conn.execute(
            "DELETE FROM topics WHERE id IN (
                SELECT id FROM topics ORDER BY weight ASC, last_seen_at ASC LIMIT ?1
            )",
            params![excess],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn top_topics(&self, limit: usize) -> Result<Vec<InterestTopic>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, label, summary, weight, last_seen_at, evidence_count, tags
                 FROM topics ORDER BY weight DESC, last_seen_at DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(InterestTopic {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    summary: row.get(2)?,
                    weight: row.get(3)?,
                    last_seen_at: parse_rfc3339(row.get::<_, String>(4)?),
                    evidence_count: row.get::<_, i64>(5)? as u32,
                    tags: parse_tags(row.get::<_, String>(6)?),
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn score_for_query(&self, query: &str, limit: usize) -> Result<Vec<InterestTopic>, String> {
        let all = self.top_topics(self.config.max_topics as usize)?;
        let q = query.to_ascii_lowercase();
        let q_tokens: Vec<&str> = q
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() >= 3)
            .collect();
        if q_tokens.is_empty() {
            return Ok(all.into_iter().take(limit).collect());
        }
        let mut scored: Vec<(f64, InterestTopic)> = all
            .into_iter()
            .map(|topic| {
                let hay = format!(
                    "{} {} {}",
                    topic.label.to_ascii_lowercase(),
                    topic.summary.to_ascii_lowercase(),
                    topic.tags.join(" ").to_ascii_lowercase()
                );
                let mut overlap = 0usize;
                for tok in &q_tokens {
                    if hay.contains(tok) {
                        overlap += 1;
                    }
                }
                let lexical = overlap as f64 / q_tokens.len() as f64;
                let score = topic.weight * (0.35 + 0.65 * lexical);
                (score, topic)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(limit).map(|(_, t)| t).collect())
    }

    pub fn render_snapshot_block(&self) -> Option<String> {
        let top_k = self.config.snapshot_top_k.max(1) as usize;
        let budget = self.config.char_budget_snapshot.max(120);
        let topics = self.top_topics(top_k).ok()?;
        self.render_block(
            "USER INTERESTS (topics this user often works on)",
            &topics,
            budget,
        )
    }

    pub fn render_prefetch_block(&self, query: &str) -> Option<String> {
        let top_k = self.config.prefetch_top_k.max(1) as usize;
        let budget = self.config.char_budget_prefetch.max(80);
        let topics = self.score_for_query(query, top_k).ok()?;
        if topics.is_empty() {
            return None;
        }
        self.render_block("Relevant user interests for this turn", &topics, budget)
    }

    fn render_block(&self, label: &str, topics: &[InterestTopic], char_budget: usize) -> Option<String> {
        if topics.is_empty() {
            return None;
        }
        let mut entries = Vec::new();
        let mut used = 0usize;
        for topic in topics {
            let line = if topic.summary.trim().is_empty() {
                topic.label.clone()
            } else {
                format!("{} — {}", topic.label, topic.summary)
            };
            let line_len = line.chars().count() + ENTRY_DELIMITER.chars().count();
            if used + line_len > char_budget && !entries.is_empty() {
                break;
            }
            entries.push(line);
            used += line_len;
        }
        if entries.is_empty() {
            return None;
        }
        let content = entries.join(ENTRY_DELIMITER);
        let current = content.chars().count();
        let pct = ((current * 100) / char_budget).min(100);
        Some(format!(
            "══════════════════════════════════════════════\n\
             {label} [{pct}% — {current}/{char_budget} chars]\n\
             ══════════════════════════════════════════════\n\
             {content}"
        ))
    }

    pub fn list_for_cli(&self) -> Result<Vec<InterestTopic>, String> {
        self.top_topics(self.config.max_topics as usize)
    }

    /// Remove rows that fail current POI quality filters (e.g. legacy `keyword: user`).
    pub fn prune_rejected_topics(&self) -> Result<usize, String> {
        let topics = self.list_for_cli()?;
        let ids: Vec<String> = topics
            .iter()
            .filter(|t| super::extract::is_rejected_poi_topic(&t.id, &t.label))
            .map(|t| t.id.clone())
            .collect();
        if ids.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        for id in &ids {
            conn.execute("DELETE FROM topics WHERE id = ?1", params![id])
                .map_err(|e| e.to_string())?;
        }
        Ok(ids.len())
    }
}

/// Prefer explicit interest labels; avoid replacing a specific label with a generic one.
fn merge_topic_label(old: &str, new: &str) -> String {
    let old = old.trim();
    let new = new.trim();
    if new.is_empty() {
        return old.to_string();
    }
    if old.is_empty() {
        return new.to_string();
    }
    if old == new {
        return old.to_string();
    }
    let old_declared = old.starts_with("兴趣:");
    let new_declared = new.starts_with("兴趣:");
    if new_declared && !old_declared {
        return new.to_string();
    }
    if old_declared && !new_declared {
        return old.to_string();
    }
    if new.chars().count() > old.chars().count() {
        new.to_string()
    } else {
        old.to_string()
    }
}

fn parse_rfc3339(raw: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn parse_tags(raw: String) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()
}

/// Load frozen interest snapshot for system prompt assembly.
pub fn load_interest_snapshot(
    hermes_home: Option<&str>,
    config: &InterestConfig,
) -> Option<String> {
    if !config.enabled {
        return None;
    }
    let home = hermes_home
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("HERMES_HOME").ok().map(std::path::PathBuf::from))
        .or_else(|| dirs::home_dir().map(|h| h.join(".hermes-agent-ultra")))
        .unwrap_or_else(|| std::path::PathBuf::from(".hermes-agent-ultra"));
    let db_path = home.join("interest.db");
    let store = InterestStore::open(&db_path, config.clone()).ok()?;
    store.render_snapshot_block()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ingest_two_distinct_chinese_interests() {
        use crate::user_interest::extract_signals_from_text;

        let dir = TempDir::new().unwrap();
        let db = dir.path().join("interest.db");
        let config = InterestConfig::default();
        let store = InterestStore::open(&db, config).unwrap();
        let mut batch = extract_signals_from_text("我的兴趣点是打篮球", 1.0);
        batch.extend(extract_signals_from_text("我的兴趣点还有吃鱼", 1.0));
        store.ingest_signals(&batch).unwrap();
        let topics = store.list_for_cli().unwrap();
        let interest_rows: Vec<_> = topics
            .iter()
            .filter(|t| t.id.starts_with("interest:"))
            .collect();
        assert!(
            interest_rows.len() >= 2,
            "expected >=2 interest rows, got {:?}",
            interest_rows
                .iter()
                .map(|t| (&t.id, &t.label))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn ingest_and_snapshot() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("interest.db");
        let config = InterestConfig::default();
        let store = InterestStore::open(&db, config).unwrap();
        store
            .ingest_signals(&[InterestSignal {
                id: "topic-rust".to_string(),
                label: "Rust development".to_string(),
                summary: "User works on Rust agent runtime".to_string(),
                weight_delta: 0.3,
                tags: vec!["rust".to_string()],
            }])
            .unwrap();
        let block = store.render_snapshot_block();
        assert!(block.is_some());
        assert!(block.unwrap().contains("Rust development"));
    }
}
