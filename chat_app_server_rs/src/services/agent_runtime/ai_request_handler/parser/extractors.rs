use serde_json::{json, Value};

use crate::core::messages::{extract_non_empty_text_value, flatten_text_value};

pub(in crate::services::agent_runtime::ai_request_handler) fn extract_output_text(response: &Value) -> String {
    if let Some(text) = response
        .get("output_text")
        .and_then(extract_text_delta)
        .filter(|value| !value.is_empty())
    {
        return text;
    }
    if let Some(text) = response
        .get("text")
        .and_then(extract_text_delta)
        .filter(|value| !value.is_empty())
    {
        return text;
    }

    if let Some(items) = response.get("output").and_then(|value| value.as_array()) {
        let mut output = Vec::new();

        for item in items {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");

            if item_type == "message" {
                if let Some(text) = item
                    .get("content")
                    .and_then(extract_text_delta)
                    .filter(|value| !value.is_empty())
                {
                    output.push(text);
                }
                continue;
            }

            if (item_type == "output_text" || item_type == "text")
                && item
                    .get("text")
                    .and_then(extract_text_delta)
                    .filter(|value| !value.is_empty())
                    .is_some()
            {
                output.push(
                    item.get("text")
                        .and_then(extract_text_delta)
                        .unwrap_or_default(),
                );
            }
        }

        if !output.is_empty() {
            return output.join("");
        }
    }

    String::new()
}

pub(super) fn extract_text_delta(delta: &Value) -> Option<String> {
    extract_non_empty_text_value(delta, &["text", "value", "content", "output_text", "delta"])
}

pub(super) fn extract_text_from_fields(value: &Value, fields: &[&str]) -> Option<String> {
    for key in fields {
        if let Some(inner) = value.get(*key) {
            if let Some(text) = extract_text_delta(inner) {
                return Some(text);
            }
        }
    }

    None
}

pub(super) fn extract_reasoning_event_text(event_type: &str, event: &Value) -> Option<String> {
    let is_reasoning_event = event_type.starts_with("response.reasoning")
        || event_type.starts_with("response.reasoning_text")
        || event_type.starts_with("response.reasoning_summary");

    if is_reasoning_event {
        for key in [
            "delta",
            "summary_text",
            "summary",
            "text",
            "part",
            "item",
            "content",
        ] {
            if let Some(value) = event.get(key) {
                let text = normalize_reasoning_delta(Some(value));
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }

        if let Some(response) = event.get("response") {
            let text = extract_reasoning_from_response(response);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    if event_type == "response.output_item.added"
        || event_type == "response.output_item.delta"
        || event_type == "response.output_item.done"
    {
        let item = event.get("item").or_else(|| event.get("output_item"));
        if let Some(item) = item {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if item_type == "reasoning" || item_type == "reasoning_summary" {
                let text = extract_reasoning_from_response(&json!({ "output": [item.clone()] }));
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }

    None
}

pub(in crate::services::agent_runtime::ai_request_handler) fn extract_reasoning_from_response(
    response: &Value,
) -> String {
    if let Some(reasoning) = response
        .get("reasoning")
        .or_else(|| response.get("reasoning_summary"))
    {
        let text = normalize_reasoning_delta(Some(reasoning));
        if !text.is_empty() {
            return text;
        }
    }

    let mut parts = Vec::new();
    if let Some(output_items) = response.get("output").and_then(|value| value.as_array()) {
        for item in output_items {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if item_type != "reasoning" && item_type != "reasoning_summary" {
                continue;
            }

            let mut item_has_text = false;
            for key in ["summary", "content", "text", "delta", "reasoning"] {
                if let Some(value) = item.get(key) {
                    let text = normalize_reasoning_delta(Some(value));
                    if !text.is_empty() {
                        parts.push(text);
                        item_has_text = true;
                    }
                }
            }

            if !item_has_text {
                let text = normalize_reasoning_delta(Some(item));
                if !text.is_empty() {
                    parts.push(text);
                }
            }
        }
    }

    parts.join("")
}

pub(super) fn looks_like_response_id(id: &str) -> bool {
    let normalized = id.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }

    if normalized.starts_with("event_") || normalized.starts_with("call_") {
        return false;
    }

    if normalized.starts_with("resp_")
        || normalized.starts_with("response_")
        || normalized.starts_with("chatcmpl-")
        || normalized.starts_with("cmpl-")
    {
        return true;
    }

    normalized.len() >= 16
}

fn extract_reasoning_text(value: &Value) -> String {
    flatten_text_value(
        value,
        &[
            "text",
            "summary_text",
            "delta",
            "content",
            "summary",
            "reasoning",
            "reasoning_text",
            "value",
            "part",
            "item",
        ],
    )
}

fn normalize_reasoning_delta(delta: Option<&Value>) -> String {
    delta.map(extract_reasoning_text).unwrap_or_default()
}
