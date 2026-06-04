use serde_json::{json, Value};
use uuid::Uuid;

pub fn task_id() -> String {
    Uuid::new_v4().to_string().replace('-', "")
}

pub fn header_field(msg: &Value, key: &str) -> Option<String> {
    msg.get("header")?
        .get(key)?
        .as_str()
        .map(str::to_string)
}

pub fn event_name(msg: &Value) -> Option<String> {
    header_field(msg, "event")
}

pub fn run_task_asr(task_id: &str, model: &str, sample_rate: u32, format: &str) -> Value {
    json!({
        "header": {
            "action": "run-task",
            "task_id": task_id,
            "streaming": "duplex"
        },
        "payload": {
            "task_group": "audio",
            "task": "asr",
            "function": "recognition",
            "model": model,
            "parameters": {
                "sample_rate": sample_rate,
                "format": format
            },
            "input": {}
        }
    })
}

pub fn run_task_tts(
    task_id: &str,
    model: &str,
    voice: &str,
    sample_rate: u32,
    format: &str,
) -> Value {
    json!({
        "header": {
            "action": "run-task",
            "task_id": task_id,
            "streaming": "duplex"
        },
        "payload": {
            "task_group": "audio",
            "task": "tts",
            "function": "SpeechSynthesizer",
            "model": model,
            "parameters": {
                "text_type": "PlainText",
                "voice": voice,
                "format": format,
                "sample_rate": sample_rate,
                "volume": 50,
                "rate": 1,
                "pitch": 1
            },
            "input": {}
        }
    })
}

pub fn continue_task(task_id: &str, text: &str) -> Value {
    json!({
        "header": {
            "action": "continue-task",
            "task_id": task_id,
            "streaming": "duplex"
        },
        "payload": {
            "input": {
                "text": text
            }
        }
    })
}

pub fn finish_task(task_id: &str) -> Value {
    json!({
        "header": {
            "action": "finish-task",
            "task_id": task_id,
            "streaming": "duplex"
        },
        "payload": {
            "input": {}
        }
    })
}
