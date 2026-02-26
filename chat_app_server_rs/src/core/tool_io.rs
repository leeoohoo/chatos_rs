use serde_json::{json, Value};

pub fn text_result(payload: Value) -> Value {
    let text = if payload.is_string() {
        payload.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
    };

    json!({
        "content": [
            { "type": "text", "text": text }
        ]
    })
}
