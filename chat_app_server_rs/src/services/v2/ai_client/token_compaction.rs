use serde_json::Value;

pub(super) fn estimate_delta_stats(messages: &[Value]) -> (i64, i64) {
    let mut tokens = 0i64;
    let mut count = 0i64;

    for message in messages {
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if role != "system" && role != "user" {
            count += 1;
        }

        let content = message.get("content").unwrap_or(&Value::Null);
        tokens += estimate_tokens_value(content);
    }

    (tokens, count)
}

fn estimate_tokens_plain(text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }

    ((text.len() as i64) + 3) / 4
}

fn estimate_tokens_value(content: &Value) -> i64 {
    if let Some(text) = content.as_str() {
        return estimate_tokens_plain(text);
    }

    if let Some(array) = content.as_array() {
        let mut sum = 0i64;

        for part in array {
            if let Some(text) = part.as_str() {
                sum += estimate_tokens_plain(text);
                continue;
            }

            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                if let Some(part_type) = part.get("type").and_then(|value| value.as_str()) {
                    if part_type == "text"
                        || part_type == "input_text"
                        || part_type == "output_text"
                    {
                        sum += estimate_tokens_plain(text);
                        continue;
                    }
                }
                sum += estimate_tokens_plain(text);
            }
        }

        return sum;
    }

    if let Some(object) = content.as_object() {
        if let Some(text) = object.get("text").and_then(|value| value.as_str()) {
            return estimate_tokens_plain(text);
        }

        return estimate_tokens_plain(&content.to_string());
    }

    0
}

fn estimate_message_tokens(message: &Value) -> i64 {
    let mut tokens = estimate_tokens_value(message.get("content").unwrap_or(&Value::Null));
    if let Some(tool_calls) = message.get("tool_calls") {
        tokens += estimate_tokens_plain(&tool_calls.to_string());
    }

    tokens
}

fn extract_error_message(err: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(err) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(|value| value.as_str())
        {
            return message.to_string();
        }
        if let Some(message) = value.get("message").and_then(|value| value.as_str()) {
            return message.to_string();
        }
    }

    err.to_string()
}

pub(super) fn is_token_limit_error(err: &str) -> bool {
    let message = extract_error_message(err).to_lowercase();
    message.contains("token limit")
        || message.contains("context length")
        || message.contains("maximum context")
        || (message.contains("exceeded") && message.contains("token"))
}

fn parse_number_after(text: &str, key: &str) -> Option<i64> {
    let lower = text.to_lowercase();
    let index = lower.find(key)?;
    let tail = &lower[index + key.len()..];
    let digits: String = tail
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

pub(super) fn token_limit_budget_from_error(err: &str) -> Option<i64> {
    let message = extract_error_message(err);
    let lower = message.to_lowercase();
    if !(lower.contains("token limit")
        || lower.contains("context length")
        || lower.contains("maximum context"))
    {
        return None;
    }

    let limit = parse_number_after(&message, "limit")
        .or_else(|| parse_number_after(&message, "context length"))
        .or_else(|| parse_number_after(&message, "maximum context"));

    limit.map(|value| (value - 2048).max(1000))
}

pub(super) fn truncate_messages_by_tokens(
    messages: &[Value],
    max_tokens: i64,
) -> (Vec<Value>, bool) {
    if max_tokens <= 0 || messages.is_empty() {
        return (messages.to_vec(), false);
    }

    let mut system_prefix = Vec::new();
    let mut index = 0usize;
    while index < messages.len() {
        if messages[index].get("role").and_then(|value| value.as_str()) == Some("system") {
            system_prefix.push(messages[index].clone());
            index += 1;
            continue;
        }
        break;
    }

    let mut tokens: i64 = system_prefix.iter().map(estimate_message_tokens).sum();
    if tokens >= max_tokens {
        let truncated = truncate_messages_content_only(&system_prefix, max_tokens);
        return (truncated, true);
    }

    let mut tail_reversed: Vec<Value> = Vec::new();
    for message in messages[index..].iter().rev() {
        let token_count = estimate_message_tokens(message);
        if tokens + token_count > max_tokens {
            if tail_reversed.is_empty() {
                let remaining = max_tokens - tokens;
                if remaining > 0 {
                    tail_reversed.push(truncate_message_content(message, remaining));
                }
            }
            break;
        }

        tokens += token_count;
        tail_reversed.push(message.clone());
    }
    tail_reversed.reverse();

    let mut output = system_prefix;
    output.extend(tail_reversed);
    let truncated = output.len() < messages.len();
    (output, truncated)
}

fn truncate_messages_content_only(messages: &[Value], max_tokens: i64) -> Vec<Value> {
    let mut output = Vec::new();
    let mut remaining = max_tokens;

    for message in messages {
        if remaining <= 0 {
            break;
        }

        let token_count = estimate_message_tokens(message);
        if token_count <= remaining {
            remaining -= token_count;
            output.push(message.clone());
            continue;
        }

        output.push(truncate_message_content(message, remaining));
        break;
    }

    output
}

fn truncate_message_content(message: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 {
        return message.clone();
    }

    let mut output = message.clone();
    if let Some(map) = output.as_object_mut() {
        let content = map.get("content").cloned().unwrap_or(Value::Null);
        let truncated = truncate_content_value(&content, max_tokens);
        map.insert("content".to_string(), truncated);
    }

    output
}

fn truncate_content_value(content: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 {
        return Value::String(String::new());
    }

    if let Some(text) = content.as_str() {
        return Value::String(truncate_text_by_tokens(text, max_tokens));
    }

    if let Some(array) = content.as_array() {
        let mut output = Vec::new();
        let mut remaining = max_tokens;

        for part in array {
            if remaining <= 0 {
                break;
            }

            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                let truncated = truncate_text_by_tokens(text, remaining);
                let used = estimate_tokens_plain(&truncated);
                let mut new_part = part.clone();
                if let Some(map) = new_part.as_object_mut() {
                    map.insert("text".to_string(), Value::String(truncated));
                }
                output.push(new_part);
                remaining -= used;
                continue;
            }

            if let Some(text) = part.as_str() {
                let truncated = truncate_text_by_tokens(text, remaining);
                let used = estimate_tokens_plain(&truncated);
                output.push(Value::String(truncated));
                remaining -= used;
                continue;
            }

            output.push(part.clone());
        }

        return Value::Array(output);
    }

    Value::String(truncate_text_by_tokens(&content.to_string(), max_tokens))
}

fn truncate_text_by_tokens(text: &str, max_tokens: i64) -> String {
    if max_tokens <= 0 {
        return String::new();
    }

    let max_chars = (max_tokens * 4) as usize;
    if text.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }

    let marker = "\n...[truncated]";
    if max_chars <= marker.len() {
        return marker[..max_chars].to_string();
    }

    let cut = max_chars - marker.len();
    format!("{}{}", &text[..cut], marker)
}
