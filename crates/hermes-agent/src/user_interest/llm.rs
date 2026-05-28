//! Optional LLM-based topic extraction for user interests.

use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};

use super::extract::parse_llm_topics_json;
use super::store::InterestSignal;

#[derive(Clone)]
struct SummaryClient {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

fn summary_client_from_env() -> Option<SummaryClient> {
    let model = std::env::var("HERMES_INTEREST_SUMMARY_MODEL")
        .ok()
        .or_else(|| std::env::var("HERMES_SESSION_SEARCH_SUMMARY_MODEL").ok())
        .or_else(|| std::env::var("HERMES_MODEL").ok())
        .unwrap_or_else(|| "gpt-4o-mini".to_string());
    let model = model
        .split(':')
        .next_back()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("gpt-4o-mini")
        .to_string();

    let base_url = std::env::var("HERMES_INTEREST_SUMMARY_BASE_URL")
        .ok()
        .or_else(|| std::env::var("HERMES_SESSION_SEARCH_SUMMARY_BASE_URL").ok())
        .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
        .trim()
        .trim_end_matches('/')
        .to_string();

    let mut api_key = std::env::var("HERMES_INTEREST_SUMMARY_API_KEY")
        .ok()
        .or_else(|| std::env::var("HERMES_SESSION_SEARCH_SUMMARY_API_KEY").ok())
        .or_else(|| std::env::var("HERMES_OPENAI_API_KEY").ok())
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .unwrap_or_default();
    if api_key.trim().is_empty() && base_url.to_lowercase().contains("openrouter.ai") {
        api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
    }
    if api_key.trim().is_empty() {
        return None;
    }

    Some(SummaryClient {
        client: Client::new(),
        base_url,
        api_key,
        model,
    })
}

/// Extract interest topics from a conversation transcript via LLM.
pub async fn extract_signals_from_transcript_llm(transcript: &str) -> Vec<InterestSignal> {
    let Some(client) = summary_client_from_env() else {
        return Vec::new();
    };
    let trimmed = transcript.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let max_chars = 12_000usize;
    let body = if trimmed.chars().count() > max_chars {
        format!(
            "{}\n…[truncated]",
            trimmed.chars().take(max_chars).collect::<String>()
        )
    } else {
        trimmed.to_string()
    };

    let system = "You extract durable user interest topics from agent conversations. \
                  Output ONLY a JSON array (no markdown). Each item: \
                  {\"label\": string, \"summary\": string, \"confidence\": 0-1, \"tags\": [string]}. \
                  Max 3 items. Focus on recurring goals, tech stacks, projects — not one-off chit-chat.";
    let user = format!(
        "Extract up to 3 user interest topics from this transcript:\n\n{body}"
    );

    let url = format!("{}/chat/completions", client.base_url);
    for attempt in 0..3 {
        let request_body = json!({
            "model": client.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.1,
            "max_tokens": 800,
        });
        let mut req = client
            .client
            .post(&url)
            .bearer_auth(client.api_key.trim())
            .timeout(Duration::from_secs(60))
            .json(&request_body);
        if client.base_url.to_lowercase().contains("openrouter.ai") {
            req = req
                .header("HTTP-Referer", "https://hermes-agent.nousresearch.com")
                .header("X-OpenRouter-Title", "Hermes Agent");
        }
        if let Ok(resp) = req.send().await {
            if let Ok(v) = resp.json::<Value>().await {
                let text = v
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|x| x.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let parsed = parse_llm_topics_json(text);
                if !parsed.is_empty() {
                    return parsed;
                }
            }
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_secs((attempt + 1) as u64)).await;
        }
    }
    Vec::new()
}
