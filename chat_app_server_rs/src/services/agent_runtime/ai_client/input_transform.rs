use serde_json::{json, Value};

use crate::core::messages::text_value_or_json;

pub(super) fn extract_raw_input(messages: &[Value]) -> Value {
    if let Some(last_user) = messages
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(|value| value.as_str()) == Some("user"))
    {
        if let Some(content) = last_user.get("content") {
            return convert_parts_to_response_input(content);
        }
    }

    if let Some(last) = messages.last() {
        if let Some(content) = last.get("content") {
            return convert_parts_to_response_input(content);
        }
    }

    Value::String(String::new())
}

fn convert_parts_to_response_input(content: &Value) -> Value {
    if let Some(text) = content.as_str() {
        return Value::String(text.to_string());
    }

    if let Some(parts) = content.as_array() {
        let mut content_list = Vec::new();

        for part in parts {
            if let Some(part_type) = part.get("type").and_then(|value| value.as_str()) {
                if part_type == "input_text" {
                    if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                        content_list.push(json!({"type": "input_text", "text": text}));
                        continue;
                    }
                }

                if part_type == "input_image" {
                    let image_url = part.get("image_url").cloned().unwrap_or(Value::Null);
                    let file_id = part.get("file_id").cloned().unwrap_or(Value::Null);
                    let detail = part
                        .get("detail")
                        .cloned()
                        .unwrap_or(Value::String("auto".to_string()));
                    content_list.push(json!({
                        "type": "input_image",
                        "image_url": image_url,
                        "file_id": file_id,
                        "detail": detail
                    }));
                    continue;
                }

                if part_type == "text" {
                    if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                        content_list.push(json!({"type": "input_text", "text": text}));
                        continue;
                    }
                }

                if part_type == "image_url" {
                    let url = image_part_locator(part);
                    content_list.push(json!({
                        "type": "input_image",
                        "image_url": url,
                        "detail": part
                            .get("detail")
                            .cloned()
                            .unwrap_or(Value::String("auto".to_string()))
                    }));
                    continue;
                }
            }

            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                content_list.push(json!({"type": "input_text", "text": text}));
                continue;
            }

            content_list.push(json!({
                "type": "input_text",
                "text": text_value_or_json(part, &["text", "value", "content", "delta"])
            }));
        }

        return Value::Array(vec![json!({
            "role": "user",
            "content": content_list,
            "type": "message"
        })]);
    }

    Value::String(content.to_string())
}

pub(super) fn to_message_item(role: &str, content: &Value, force_text_content: bool) -> Value {
    to_message_item_with_reasoning(role, content, None, force_text_content)
}

pub(super) fn to_message_item_with_reasoning(
    role: &str,
    content: &Value,
    reasoning: Option<&str>,
    force_text_content: bool,
) -> Value {
    if force_text_content {
        return json!({
            "role": role,
            "content": content_parts_to_text(content),
            "type": "message"
        });
    }

    if role == "assistant" {
        let mut content_items = Vec::new();
        if let Some(reasoning_text) = reasoning
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            content_items.push(json!({
                "type": "reasoning",
                "text": reasoning_text
            }));
        }
        content_items.push(json!({
            "type": "output_text",
            "text": content_parts_to_text(content)
        }));
        return json!({
            "role": role,
            "content": content_items,
            "type": "message"
        });
    }

    if content.is_array() {
        return json!({"role": role, "content": content.clone(), "type": "message"});
    }

    json!({
        "role": role,
        "content": to_input_text_content(content_parts_to_text(content)),
        "type": "message"
    })
}

fn to_input_text_content(text: String) -> Value {
    Value::Array(vec![json!({"type": "input_text", "text": text})])
}

fn image_part_locator(part: &Value) -> &str {
    part.get("image_url")
        .and_then(|value| value.get("url"))
        .and_then(|value| value.as_str())
        .or_else(|| part.get("image_url").and_then(|value| value.as_str()))
        .or_else(|| part.get("file_id").and_then(|value| value.as_str()))
        .unwrap_or("")
}

fn content_parts_to_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(parts) = content.as_array() {
        let mut output = Vec::new();

        for part in parts {
            if let Some(text) = part.as_str() {
                output.push(text.to_string());
                continue;
            }

            if let Some(part_type) = part.get("type").and_then(|value| value.as_str()) {
                if (part_type == "input_text" || part_type == "output_text" || part_type == "text")
                    && part.get("text").and_then(|value| value.as_str()).is_some()
                {
                    output.push(
                        part.get("text")
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                    continue;
                }

                if part_type == "input_image" || part_type == "image_url" {
                    let url = image_part_locator(part);
                    output.push(if url.is_empty() {
                        "[image]".to_string()
                    } else {
                        format!("[image:{}]", url)
                    });
                    continue;
                }
            }

            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                output.push(text.to_string());
                continue;
            }

            output.push(text_value_or_json(
                part,
                &["text", "value", "content", "delta"],
            ));
        }

        return output.join("\n");
    }

    text_value_or_json(content, &["text", "value", "content", "delta"])
}

pub(super) fn normalize_input_to_text_value(input: &Value) -> Value {
    if let Some(items) = input.as_array() {
        let mapped: Vec<Value> = items
            .iter()
            .map(|item| {
                if item.get("type").and_then(|value| value.as_str()) == Some("message") {
                    let content = item.get("content").cloned().unwrap_or(Value::Null);
                    let mut cloned = item.clone();
                    if let Some(map) = cloned.as_object_mut() {
                        map.insert(
                            "content".to_string(),
                            Value::String(content_parts_to_text(&content)),
                        );
                    }
                    return cloned;
                }
                item.clone()
            })
            .collect();
        return Value::Array(mapped);
    }

    input.clone()
}

pub(super) fn normalize_input_for_provider(input: &Value, force_text: bool) -> Value {
    if force_text {
        normalize_input_to_text_value(input)
    } else {
        input.clone()
    }
}

pub(super) fn build_current_input_items(raw_input: &Value, force_text: bool) -> Vec<Value> {
    let normalized = normalize_input_for_provider(raw_input, force_text);
    if let Some(items) = normalized.as_array() {
        return items.clone();
    }

    vec![to_message_item("user", &normalized, force_text)]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{content_parts_to_text, convert_parts_to_response_input};

    #[test]
    fn content_parts_to_text_preserves_image_placeholders_and_text_fallback() {
        let content = json!([
            {"type": "input_text", "text": "hello"},
            {"type": "input_image", "image_url": {"url": "https://img.example/a.png"}},
            {"type": "image_url", "image_url": "https://img.example/b.png"},
            {"other": 1}
        ]);

        assert_eq!(
            content_parts_to_text(&content),
            "hello\n[image:https://img.example/a.png]\n[image:https://img.example/b.png]\n{\"other\":1}"
        );
    }

    #[test]
    fn convert_parts_to_response_input_normalizes_text_and_images() {
        let content = json!([
            {"type": "text", "text": "hello"},
            {"type": "image_url", "image_url": {"url": "https://img.example/a.png"}, "detail": "high"},
            {"other": 1}
        ]);

        let normalized = convert_parts_to_response_input(&content);
        assert_eq!(
            normalized,
            json!([{
                "role": "user",
                "type": "message",
                "content": [
                    {"type": "input_text", "text": "hello"},
                    {"type": "input_image", "image_url": "https://img.example/a.png", "detail": "high"},
                    {"type": "input_text", "text": "{\"other\":1}"}
                ]
            }])
        );
    }
}
