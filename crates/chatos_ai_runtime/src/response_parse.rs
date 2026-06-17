use serde_json::Value;

pub fn extract_output_text(response: &Value) -> String {
    if let Some(value) = response.get("output_text") {
        if let Some(text) = text_value_or_json(
            value,
            &["text", "value", "content", "output_text", "delta", "output"],
        ) {
            if !text.is_empty() {
                return text;
            }
        }
    }
    response
        .get("output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(Value::as_str) == Some("message") {
                        item.get("content").map(chat_message_content_to_text)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

pub fn extract_reasoning_from_response(response: &Value) -> String {
    response
        .get("output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item.get("type").and_then(Value::as_str) {
                    Some("reasoning") | Some("reasoning_summary") => Some(
                        item.get("summary")
                            .or_else(|| item.get("text"))
                            .or_else(|| item.get("content"))
                            .map(chat_message_content_to_text)
                            .unwrap_or_else(|| chat_message_content_to_text(item)),
                    ),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

pub fn chat_message_content_to_text(content: &Value) -> String {
    join_text_lines_or_json(
        content,
        &["text", "value", "content", "delta", "output_text", "output"],
    )
}

pub fn text_value_or_json(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Bool(_) | Value::Number(_) => Some(value.to_string()),
        Value::Array(_) => {
            let text = join_text_lines_or_json(value, keys);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        Value::Object(map) => {
            for key in keys {
                if let Some(child) = map.get(*key) {
                    if let Some(text) = text_value_or_json(child, keys) {
                        if !text.is_empty() {
                            return Some(text);
                        }
                    }
                }
            }
            Some(value.to_string())
        }
    }
}

pub fn join_text_lines_or_json(value: &Value, keys: &[&str]) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
        Value::Array(items) => items
            .iter()
            .map(|item| join_text_lines_or_json(item, keys))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(""),
        Value::Object(map) => {
            for key in keys {
                if let Some(child) = map.get(*key) {
                    let text = join_text_lines_or_json(child, keys);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
            value.to_string()
        }
    }
}

pub fn tool_arguments_to_string(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

pub fn append_stream_text(current: &mut String, chunk: &str) {
    *current = join_stream_text(current.as_str(), chunk);
}

pub fn join_stream_text(current: &str, chunk: &str) -> String {
    join_stream_text_with_min_overlap(current, chunk, 1)
}

pub fn join_stream_text_with_min_overlap(
    current: &str,
    chunk: &str,
    min_overlap: usize,
) -> String {
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
    let min_overlap = min_overlap.max(1);
    for overlap in (min_overlap..=max_overlap).rev() {
        let Some(current_tail) = current.get(current.len() - overlap..) else {
            continue;
        };
        let Some(chunk_head) = chunk.get(..overlap) else {
            continue;
        };
        if current_tail == chunk_head {
            let rest = chunk.get(overlap..).unwrap_or_default();
            return format!("{current}{rest}");
        }
    }
    format!("{current}{chunk}")
}

pub fn looks_like_response_id(value: &str) -> bool {
    let normalized = value.trim().to_lowercase();
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        extract_output_text, extract_reasoning_from_response, join_stream_text,
        join_stream_text_with_min_overlap, looks_like_response_id, tool_arguments_to_string,
    };

    #[test]
    fn extract_output_text_reads_output_message_parts() {
        let response = json!({
            "output": [
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "Hello"},
                        {"type": "text", "text": " world"}
                    ]
                }
            ]
        });

        assert_eq!(extract_output_text(&response), "Hello world");
    }

    #[test]
    fn extract_reasoning_from_response_collects_reasoning_summary_items() {
        let response = json!({
            "output": [
                {
                    "type": "reasoning",
                    "summary": [{"text": "step-1"}]
                },
                {
                    "type": "reasoning_summary",
                    "text": "step-2"
                }
            ]
        });

        assert_eq!(extract_reasoning_from_response(&response), "step-1step-2");
    }

    #[test]
    fn looks_like_response_id_filters_event_ids_and_accepts_response_prefix() {
        assert!(looks_like_response_id("resp_abc"));
        assert!(looks_like_response_id("chatcmpl-123"));
        assert!(!looks_like_response_id("event_123"));
        assert!(!looks_like_response_id("call_123"));
        assert!(!looks_like_response_id("   "));
    }

    #[test]
    fn join_stream_text_handles_unicode_snapshot_overlap() {
        assert_eq!(
            join_stream_text("你好世界ABCD", "好世界ABCD123"),
            "你好世界ABCD123"
        );
    }

    #[test]
    fn join_stream_text_with_min_overlap_preserves_threshold() {
        assert_eq!(
            join_stream_text_with_min_overlap("abcdef", "def123", 3),
            "abcdef123"
        );
        assert_eq!(
            join_stream_text_with_min_overlap("abcdef", "def123", 4),
            "abcdefdef123"
        );
    }

    #[test]
    fn tool_arguments_to_string_serializes_object_values() {
        assert_eq!(
            tool_arguments_to_string(&json!({"q":"rust"})),
            "{\"q\":\"rust\"}"
        );
    }
}
