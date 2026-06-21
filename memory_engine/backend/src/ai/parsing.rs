use serde_json::Value;

#[cfg(test)]
pub(crate) fn extract_chat_completion_text(value: &Value) -> Option<String> {
    let message = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))?;

    extract_textish_value(message.get("content")?)
}

pub(crate) fn extract_chat_completion_stream_text(value: &Value) -> Option<String> {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())?;

    if let Some(content) = choice.get("delta").and_then(|delta| delta.get("content")) {
        if let Some(text) = extract_textish_value(content) {
            return Some(text);
        }
    }

    choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(extract_textish_value)
}

pub(crate) fn extract_responses_output_text(value: &Value) -> Option<String> {
    if let Some(text) = value
        .get("output_text")
        .and_then(Value::as_str)
        .and_then(trimmed_non_empty)
    {
        return Some(text);
    }

    let mut parts = Vec::new();
    let items = value.get("output").and_then(Value::as_array)?;

    for item in items {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
        if item_type == "message" {
            if let Some(contents) = item.get("content").and_then(Value::as_array) {
                for content in contents {
                    let content_type = content.get("type").and_then(Value::as_str).unwrap_or("");
                    if content_type == "output_text"
                        || content_type == "input_text"
                        || content_type == "text"
                    {
                        if let Some(text) = content
                            .get("text")
                            .and_then(Value::as_str)
                            .and_then(trimmed_non_empty)
                        {
                            parts.push(text);
                        }
                    }
                }
            }
            continue;
        }

        if (item_type == "output_text" || item_type == "input_text" || item_type == "text")
            && item.get("text").and_then(Value::as_str).is_some()
        {
            if let Some(text) = item
                .get("text")
                .and_then(Value::as_str)
                .and_then(trimmed_non_empty)
            {
                parts.push(text);
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

pub(crate) fn extract_responses_stream_text(
    value: &Value,
    already_streamed_text: bool,
) -> Option<String> {
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    match event_type {
        "response.output_text.delta" => value
            .get("delta")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        "response.completed" if !already_streamed_text => value
            .get("response")
            .and_then(extract_responses_output_text),
        "response.output_text.done" if !already_streamed_text => value
            .get("text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        _ if !already_streamed_text => value
            .get("response")
            .and_then(extract_responses_output_text)
            .or_else(|| extract_responses_output_text(value)),
        _ => None,
    }
}

pub(crate) fn extract_stream_error_message(value: &Value) -> Option<String> {
    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .and_then(trimmed_non_empty)
    {
        return Some(message);
    }

    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    if event_type == "error" || event_type == "response.failed" {
        return value
            .get("message")
            .and_then(Value::as_str)
            .and_then(trimmed_non_empty)
            .or_else(|| {
                value
                    .get("response")
                    .and_then(|response| response.get("error"))
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
            })
            .or_else(|| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
            });
    }

    None
}

pub(crate) fn trim_or_truncate_for_log(raw: &str, max_chars: usize) -> String {
    let chars = raw.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        raw.to_string()
    } else {
        let prefix = chars[..max_chars].iter().collect::<String>();
        format!("{prefix}...<truncated>")
    }
}

pub(crate) fn trimmed_non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_textish_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str().and_then(trimmed_non_empty) {
        return Some(text);
    }

    let array = value.as_array()?;
    let mut parts = Vec::new();
    for item in array {
        if let Some(text) = item
            .get("text")
            .and_then(Value::as_str)
            .and_then(trimmed_non_empty)
        {
            parts.push(text);
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}
