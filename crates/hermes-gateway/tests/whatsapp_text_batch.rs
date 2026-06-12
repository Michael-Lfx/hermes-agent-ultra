#![cfg(feature = "whatsapp")]

//! WhatsApp text batch debounce tests.

use hermes_gateway::gateway::IncomingMessage;
use hermes_gateway::platforms::whatsapp::{TextBatchState, WhatsAppConfig, batch_key};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn rapid_texts_collapse() {
    let cfg = WhatsAppConfig {
        text_batch_delay_seconds: 0.05,
        ..WhatsAppConfig::default()
    };
    let state = TextBatchState::new(&cfg);
    let dispatched: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let d1 = dispatched.clone();
    let d2 = dispatched.clone();
    state
        .enqueue(
            batch_key(&IncomingMessage::new("whatsapp", "c1", "u1", "one", true)),
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
            batch_key(&IncomingMessage::new("whatsapp", "c1", "u1", "two", true)),
            IncomingMessage::new("whatsapp", "c1", "u1", "two", true),
            move |msg| {
                let d = d2.clone();
                async move {
                    d.lock().await.push(msg.text);
                }
            },
        )
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let texts = dispatched.lock().await.clone();
    assert_eq!(texts.len(), 1);
    assert!(texts[0].contains("one"));
    assert!(texts[0].contains("two"));
}
