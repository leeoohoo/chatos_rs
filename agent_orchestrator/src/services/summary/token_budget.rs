use serde_json::Value;

fn estimate_tokens(text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }
    ((text.len() as i64) + 3) / 4
}

pub fn estimate_tokens_value(content: &Value) -> i64 {
    if let Some(text) = content.as_str() {
        return estimate_tokens(text);
    }

    if let Some(array) = content.as_array() {
        let mut sum = 0i64;
        for part in array {
            if let Some(text) = part.as_str() {
                sum += estimate_tokens(text);
                continue;
            }
            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                sum += estimate_tokens(text);
                continue;
            }
            sum += estimate_tokens(&part.to_string());
        }
        return sum;
    }

    if let Some(object) = content.as_object() {
        if let Some(text) = object.get("text").and_then(|value| value.as_str()) {
            return estimate_tokens(text);
        }
        return estimate_tokens(&content.to_string());
    }

    0
}

pub fn estimate_message_tokens(message: &Value) -> i64 {
    let mut tokens = estimate_tokens_value(message.get("content").unwrap_or(&Value::Null));
    if let Some(tool_calls) = message.get("tool_calls") {
        tokens += estimate_tokens(&tool_calls.to_string());
    }
    tokens
}

pub fn estimate_messages_tokens(messages: &[Value]) -> i64 {
    messages.iter().map(estimate_message_tokens).sum()
}

pub fn truncate_messages_by_tokens(messages: &[Value], max_tokens: i64) -> Vec<Value> {
    if max_tokens <= 0 || messages.is_empty() {
        return Vec::new();
    }

    let mut remaining = max_tokens;
    let mut out_rev = Vec::new();

    for message in messages.iter().rev() {
        let token_count = estimate_message_tokens(message);
        if remaining - token_count < 0 {
            if out_rev.is_empty() && remaining > 0 {
                out_rev.push(truncate_message_content(message, remaining));
            }
            break;
        }

        out_rev.push(message.clone());
        remaining -= token_count;
    }

    out_rev.reverse();
    out_rev
}

pub fn truncate_message_content(message: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 {
        return message.clone();
    }

    let mut out = message.clone();
    if let Some(map) = out.as_object_mut() {
        let content = map.get("content").cloned().unwrap_or(Value::Null);
        map.insert(
            "content".to_string(),
            truncate_content_value(&content, max_tokens),
        );
    }
    out
}

pub fn truncate_content_value(content: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 {
        return Value::String(String::new());
    }

    if let Some(text) = content.as_str() {
        return Value::String(truncate_text_by_tokens(text, max_tokens));
    }

    if let Some(array) = content.as_array() {
        let mut out = Vec::new();
        let mut remaining = max_tokens;

        for part in array {
            if remaining <= 0 {
                break;
            }

            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                let truncated = truncate_text_by_tokens(text, remaining);
                let used = estimate_tokens(&truncated);
                let mut new_part = part.clone();
                if let Some(map) = new_part.as_object_mut() {
                    map.insert("text".to_string(), Value::String(truncated));
                }
                out.push(new_part);
                remaining -= used;
                continue;
            }

            if let Some(text) = part.as_str() {
                let truncated = truncate_text_by_tokens(text, remaining);
                let used = estimate_tokens(&truncated);
                out.push(Value::String(truncated));
                remaining -= used;
                continue;
            }

            out.push(part.clone());
        }

        return Value::Array(out);
    }

    Value::String(truncate_text_by_tokens(&content.to_string(), max_tokens))
}

pub fn truncate_text_by_tokens(text: &str, max_tokens: i64) -> String {
    if max_tokens <= 0 {
        return String::new();
    }

    let max_chars = (max_tokens * 4) as usize;
    if text.len() <= max_chars {
        return text.to_string();
    }

    let marker = "\n...[truncated]";
    if max_chars <= marker.len() {
        return marker[..max_chars].to_string();
    }

    let cut = max_chars - marker.len();
    format!("{}{}", &text[..cut], marker)
}

fn extract_error_message(err: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(err) {
        if let Some(message) = value
            .get("error")
            .and_then(|v| v.get("message"))
            .and_then(|v| v.as_str())
        {
            return message.to_string();
        }
        if let Some(message) = value.get("message").and_then(|v| v.as_str()) {
            return message.to_string();
        }
    }

    err.to_string()
}

pub fn is_context_overflow_error(err: &str) -> bool {
    let message = extract_error_message(err).to_lowercase();
    message.contains("context_length_exceeded")
        || message.contains("input exceeds the context window")
        || message.contains("maximum context length")
        || message.contains("token limit")
        || message.contains("context length")
        || message.contains("maximum context")
        || (message.contains("context window") && message.contains("exceed"))
        || (message.contains("exceeded") && message.contains("token"))
}

fn parse_number_after(text: &str, key: &str) -> Option<i64> {
    let lower = text.to_lowercase();
    let index = lower.find(key)?;
    let tail = &lower[index + key.len()..];
    let digits: String = tail
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok()
}

pub fn token_budget_from_context_overflow_error(err: &str) -> Option<i64> {
    let message = extract_error_message(err);
    if !is_context_overflow_error(message.as_str()) {
        return None;
    }

    let limit = parse_number_after(message.as_str(), "limit")
        .or_else(|| parse_number_after(message.as_str(), "context length"))
        .or_else(|| parse_number_after(message.as_str(), "maximum context"));

    limit.map(|value| (value - 2048).max(1000))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        estimate_messages_tokens, is_context_overflow_error,
        token_budget_from_context_overflow_error, truncate_messages_by_tokens,
    };

    #[test]
    fn estimates_tokens_for_mixed_content() {
        let messages = vec![
            json!({"role": "user", "content": "hello world"}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "line1"}, {"type": "text", "text": "line2"}]}),
        ];
        let tokens = estimate_messages_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn truncates_messages_from_tail_when_over_budget() {
        let messages = vec![
            json!({"role": "user", "content": "a".repeat(300)}),
            json!({"role": "assistant", "content": "b".repeat(300)}),
            json!({"role": "user", "content": "c".repeat(300)}),
        ];
        let truncated = truncate_messages_by_tokens(&messages, 120);
        assert!(!truncated.is_empty());
        assert!(truncated.len() <= messages.len());
    }

    #[test]
    fn detects_context_overflow_errors_from_multiple_formats() {
        assert!(is_context_overflow_error("context_length_exceeded"));
        assert!(is_context_overflow_error(
            "maximum context length is 128000"
        ));
        assert!(is_context_overflow_error(
            r#"{"error":{"message":"Token limit exceeded"}}"#
        ));
        assert!(!is_context_overflow_error("rate_limit_exceeded"));
    }

    #[test]
    fn parses_budget_from_context_limit_message() {
        let budget = token_budget_from_context_overflow_error(
            "This model's maximum context length is 128000 tokens",
        );
        assert_eq!(budget, Some(125952));
    }
}
