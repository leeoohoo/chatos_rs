use std::sync::{Arc, Mutex};

use serde_json::Value;

pub(super) fn ensure_complete_event_content(
    result: &Value,
    streamed_content: Option<&Arc<Mutex<String>>>,
) -> Value {
    let Some(streamed_content) = streamed_content else {
        return result.clone();
    };
    let streamed_text = streamed_content
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let streamed_text = normalize_streamed_text(streamed_text.as_str());
    if streamed_text.is_empty() {
        return result.clone();
    }

    let result_content = result
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let normalized_result_content = normalize_streamed_text(result_content);
    let merged_content = if normalized_result_content.is_empty() {
        streamed_text
    } else {
        join_stream_text(streamed_text.as_str(), normalized_result_content.as_str())
    };

    let mut patched = result.clone();
    if let Some(obj) = patched.as_object_mut() {
        obj.insert("content".to_string(), Value::String(merged_content));
    }
    patched
}

fn normalize_streamed_text(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace("\n\n\n\n\n\n", "\n\n\n\n")
}

pub(super) fn join_stream_text(current: &str, chunk: &str) -> String {
    if chunk.is_empty() {
        return current.to_string();
    }
    if current.is_empty() {
        return chunk.to_string();
    }

    if chunk.starts_with(current) {
        return chunk.to_string();
    }
    if current.starts_with(chunk) {
        return current.to_string();
    }

    let max_overlap = std::cmp::min(current.len(), chunk.len());
    for overlap in (8..=max_overlap).rev() {
        let Some(current_tail) = current.get(current.len() - overlap..) else {
            continue;
        };
        let Some(chunk_head) = chunk.get(..overlap) else {
            continue;
        };
        if current_tail == chunk_head {
            let rest = chunk.get(overlap..).unwrap_or_default();
            return format!("{}{}", current, rest);
        }
    }

    format!("{}{}", current, chunk)
}
