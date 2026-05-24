//! Strip multimodal parts for non-vision models — parity with Python `_prepare_messages_for_non_vision_model`.

use hermes_core::{Message, MessageRole};

/// Known vision-capable model id substrings (heuristic; extend via config later).
pub fn model_supports_vision(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    [
        "gpt-4o",
        "gpt-4.1",
        "gpt-5",
        "claude-3",
        "claude-sonnet-4",
        "claude-opus-4",
        "gemini",
        "pixtral",
        "llava",
        "qwen-vl",
        "vision",
    ]
    .iter()
    .any(|hint| m.contains(hint))
}

/// Return copies of messages with image parts removed when the model lacks vision.
pub fn strip_images_for_non_vision_model(messages: &[Message], model: &str) -> Vec<Message> {
    if model_supports_vision(model) {
        return messages.to_vec();
    }
    messages
        .iter()
        .map(|msg| {
            let mut m = msg.clone();
            if matches!(m.role, MessageRole::User | MessageRole::Tool) {
                if let Some(content) = m.content.as_deref() {
                    if content.contains("data:image") || content.contains("\"type\":\"image") {
                        m.content = Some(
                            "[Image content removed: active model does not support vision. \
                             Describe the image in text or switch to a vision-capable model.]"
                                .to_string(),
                        );
                    }
                }
            }
            m
        })
        .collect()
}
