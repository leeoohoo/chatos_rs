// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::response_parse::{extract_reasoning_from_response, text_value_or_json};

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

pub(super) fn extract_chat_delta_text(delta: &Value) -> Option<String> {
    delta
        .get("content")
        .and_then(|value| text_value_or_json(value, &["text", "value", "content", "delta"]))
        .filter(|value| !value.is_empty())
}

pub(super) fn extract_chat_reasoning_text(delta: &Value) -> Option<String> {
    delta
        .get("reasoning_content")
        .or_else(|| delta.get("reasoning"))
        .and_then(|value| text_value_or_json(value, &["text", "value", "content", "delta"]))
        .filter(|value| !value.is_empty())
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
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
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

fn flatten_text_value(value: &Value, object_keys: &[&str]) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(array) = value.as_array() {
        let mut out = Vec::new();
        for item in array {
            let text = flatten_text_value(item, object_keys);
            if !text.is_empty() {
                out.push(text);
            }
        }
        return out.join("");
    }

    let Some(object) = value.as_object() else {
        return String::new();
    };

    for key in object_keys {
        if let Some(inner) = object.get(*key) {
            let text = flatten_text_value(inner, object_keys);
            if !text.is_empty() {
                return text;
            }
        }
    }

    String::new()
}

fn extract_non_empty_text_value(value: &Value, object_keys: &[&str]) -> Option<String> {
    let text = flatten_text_value(value, object_keys);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn normalize_reasoning_delta(delta: Option<&Value>) -> String {
    delta
        .map(|value| {
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
        })
        .unwrap_or_default()
}

pub(super) fn non_empty_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == value.len() {
        Some(value.to_string())
    } else {
        Some(trimmed.to_string())
    }
}
