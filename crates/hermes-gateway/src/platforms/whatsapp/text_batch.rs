//! Text debounce batching for rapid multi-message bursts.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::gateway::IncomingMessage;

use super::config::{WhatsAppConfig, TEXT_BATCH_SPLIT_THRESHOLD};

#[derive(Clone)]
struct PendingBatch {
    incoming: IncomingMessage,
    last_chunk_len: usize,
}

pub struct TextBatchState {
    delay_seconds: f64,
    split_delay_seconds: f64,
    pending: Arc<Mutex<HashMap<String, PendingBatch>>>,
    tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl TextBatchState {
    pub fn new(cfg: &WhatsAppConfig) -> Self {
        Self {
            delay_seconds: cfg.text_batch_delay_seconds,
            split_delay_seconds: cfg.text_batch_split_delay_seconds,
            pending: Arc::new(Mutex::new(HashMap::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn enqueue<F, Fut>(
        &self,
        key: String,
        incoming: IncomingMessage,
        dispatch: F,
    ) where
        F: Fn(IncomingMessage) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let chunk_len = incoming.text.len();
        {
            let mut pending = self.pending.lock().await;
            if let Some(existing) = pending.get_mut(&key) {
                if !incoming.text.is_empty() {
                    if existing.incoming.text.is_empty() {
                        existing.incoming.text = incoming.text.clone();
                    } else {
                        existing.incoming.text =
                            format!("{}\n{}", existing.incoming.text, incoming.text);
                    }
                }
                existing
                    .incoming
                    .media_urls
                    .extend(incoming.media_urls.clone());
                existing
                    .incoming
                    .media_types
                    .extend(incoming.media_types.clone());
                existing.last_chunk_len = chunk_len;
            } else {
                pending.insert(
                    key.clone(),
                    PendingBatch {
                        incoming,
                        last_chunk_len: chunk_len,
                    },
                );
            }
        }

        if let Some(old) = self.tasks.lock().await.remove(&key) {
            old.abort();
        }

        let delay = {
            let pending = self.pending.lock().await;
            let last_len = pending
                .get(&key)
                .map(|p| p.last_chunk_len)
                .unwrap_or(0);
            if last_len >= TEXT_BATCH_SPLIT_THRESHOLD {
                self.split_delay_seconds
            } else {
                self.delay_seconds
            }
        };

        let pending_ref = self.pending.clone();
        let tasks_ref = self.tasks.clone();
        let key_clone = key.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(delay)).await;
            let event = pending_ref.lock().await.remove(&key_clone);
            if let Some(batch) = event {
                dispatch(batch.incoming).await;
            }
            tasks_ref.lock().await.remove(&key_clone);
        });
        self.tasks.lock().await.insert(key, handle);
    }

    pub async fn flush_all<F, Fut>(&self, dispatch: F)
    where
        F: Fn(IncomingMessage) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        for handle in self.tasks.lock().await.drain() {
            handle.1.abort();
        }
        let pending: Vec<_> = self.pending.lock().await.drain().map(|(_, v)| v.incoming).collect();
        for incoming in pending {
            dispatch(incoming).await;
        }
    }
}

pub fn batch_key(incoming: &IncomingMessage) -> String {
    format!("{}:{}:{}", incoming.platform, incoming.chat_id, incoming.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn rapid_texts_collapse() {
        let cfg = WhatsAppConfig {
            text_batch_delay_seconds: 0.05,
            text_batch_split_delay_seconds: 0.05,
            ..WhatsAppConfig::default()
        };
        let state = TextBatchState::new(&cfg);
        let dispatched = Arc::new(Mutex::new(Vec::new()));
        let d1 = dispatched.clone();
        let d2 = dispatched.clone();
        state
            .enqueue(
                "k".into(),
                IncomingMessage::new("whatsapp", "c1", "u1", "one", true),
                move |msg| {
                    let d = d1.clone();
                    async move {
                        d.lock().await.push(msg.text);
                    }
                },
            )
            .await;
        state
            .enqueue(
                "k".into(),
                IncomingMessage::new("whatsapp", "c1", "u1", "two", true),
                move |msg| {
                    let d = d2.clone();
                    async move {
                        d.lock().await.push(msg.text);
                    }
                },
            )
            .await;
        tokio::time::sleep(Duration::from_millis(120)).await;
        let texts = dispatched.lock().await.clone();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0], "one\ntwo");
    }
}
