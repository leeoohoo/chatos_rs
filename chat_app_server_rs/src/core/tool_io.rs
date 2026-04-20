use serde_json::{json, Value};

pub fn text_result(payload: Value) -> Value {
    let text = if payload.is_string() {
        payload.as_str().unwrap_or("").to_string()
    } else if let Some(summary) = payload
        .get("_summary_text")
        .and_then(|value| value.as_str())
    {
        summary.to_string()
    } else {
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
    };

    let mut out = json!({
        "content": [
            { "type": "text", "text": text }
        ]
    });
    if !payload.is_string() && !payload.is_null() {
        out["_structured_result"] = payload;
    }
    out
}
