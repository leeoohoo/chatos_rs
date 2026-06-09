use serde_json::{json, Value};

use crate::response_parse::text_value_or_json;

pub fn extract_raw_input(messages: &[Value]) -> Value {
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

pub fn convert_parts_to_response_input(content: &Value) -> Value {
    if let Some(text) = content.as_str() {
        return Value::String(text.to_string());
    }

    if content.as_array().is_some() {
        return Value::Array(vec![json!({
            "role": "user",
            "content": normalize_response_content_parts(content),
            "type": "message"
        })]);
    }

    Value::String(content.to_string())
}

pub fn to_message_item(role: &str, content: &Value, force_text_content: bool) -> Value {
    to_message_item_with_reasoning(role, content, None, force_text_content)
}

pub fn to_message_item_with_reasoning(
    role: &str,
    content: &Value,
    reasoning: Option<&str>,
    force_text_content: bool,
) -> Value {
    let content_text = content_parts_to_text(content);
    if force_text_content {
        return json!({
            "role": role,
            "content": assistant_visible_text(content_text.as_str(), reasoning),
            "type": "message"
        });
    }

    if role == "assistant" {
        return json!({
            "role": role,
            "content": vec![json!({
                "type": "output_text",
                "text": assistant_visible_text(content_text.as_str(), reasoning)
            })],
            "type": "message"
        });
    }

    if content.is_array() {
        return json!({
            "role": role,
            "content": normalize_response_content_parts(content),
            "type": "message"
        });
    }

    json!({
        "role": role,
        "content": to_input_text_content(content_text),
        "type": "message"
    })
}

pub fn assistant_visible_text(content_text: &str, reasoning: Option<&str>) -> String {
    let content_text = content_text.trim();
    if !content_text.is_empty() {
        return content_text.to_string();
    }

    reasoning
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_string()
}

pub fn content_parts_to_text(content: &Value) -> String {
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
                        format!("[image:{url}]")
                    });
                    continue;
                }
            }

            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                output.push(text.to_string());
                continue;
            }

            output.push(text_or_json_string(
                part,
                &["text", "value", "content", "delta"],
            ));
        }

        return output.join("\n");
    }

    text_or_json_string(content, &["text", "value", "content", "delta"])
}

pub fn normalize_input_to_text_value(input: &Value) -> Value {
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

pub fn normalize_input_for_provider(input: &Value, force_text: bool) -> Value {
    if force_text {
        normalize_input_to_text_value(input)
    } else {
        input.clone()
    }
}

pub fn build_current_input_items(raw_input: &Value, force_text: bool) -> Vec<Value> {
    let normalized = normalize_input_for_provider(raw_input, force_text);
    if let Some(items) = normalized.as_array() {
        return items.clone();
    }

    vec![to_message_item("user", &normalized, force_text)]
}

pub fn response_content_has_image_part(content: &Value) -> bool {
    content
        .as_array()
        .map(|parts| {
            parts.iter().any(|part| {
                matches!(
                    part.get("type").and_then(|value| value.as_str()),
                    Some("image_url" | "input_image")
                )
            })
        })
        .unwrap_or(false)
}

pub fn prepend_input_items(
    input: &Value,
    prefixed_items: &[Value],
    force_text_content: bool,
) -> Value {
    if prefixed_items.is_empty() {
        return input.clone();
    }
    let mut merged = normalize_explicit_input_items(prefixed_items, force_text_content);
    merged.extend(build_current_input_items(input, force_text_content));
    Value::Array(merged)
}

pub fn append_input_items(
    input: &Value,
    appended_items: &[Value],
    force_text_content: bool,
) -> Value {
    if appended_items.is_empty() {
        return input.clone();
    }
    let mut merged = build_current_input_items(input, force_text_content);
    merged.extend(normalize_explicit_input_items(
        appended_items,
        force_text_content,
    ));
    Value::Array(merged)
}

fn normalize_explicit_input_items(items: &[Value], force_text_content: bool) -> Vec<Value> {
    if !force_text_content {
        return items.to_vec();
    }

    normalize_input_for_provider(&Value::Array(items.to_vec()), true)
        .as_array()
        .cloned()
        .unwrap_or_else(|| items.to_vec())
}

fn to_input_text_content(text: String) -> Value {
    Value::Array(vec![json!({"type": "input_text", "text": text})])
}

fn normalize_response_content_parts(content: &Value) -> Value {
    let Some(parts) = content.as_array() else {
        return to_input_text_content(content_parts_to_text(content));
    };

    Value::Array(parts.iter().map(normalize_response_content_part).collect())
}

fn normalize_response_content_part(part: &Value) -> Value {
    let Some(part_type) = part.get("type").and_then(|value| value.as_str()) else {
        return json!({
            "type": "input_text",
            "text": text_or_json_string(part, &["text", "value", "content", "delta"])
        });
    };

    match part_type {
        "input_text" | "output_text" | "text" => json!({
            "type": "input_text",
            "text": part
                .get("text")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| text_or_json_string(part, &["text", "value", "content", "delta"]))
        }),
        "input_image" => normalize_input_image_part(part),
        "image_url" => normalize_image_url_part(part),
        "refusal" | "summary_text" | "input_file" | "computer_screenshot" => part.clone(),
        _ => json!({
            "type": "input_text",
            "text": text_or_json_string(part, &["text", "value", "content", "delta"])
        }),
    }
}

fn normalize_input_image_part(part: &Value) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), Value::String("input_image".to_string()));
    if let Some(image_url) = response_image_url_value(part) {
        map.insert("image_url".to_string(), Value::String(image_url));
    }
    if let Some(file_id) = part
        .get("file_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        map.insert("file_id".to_string(), Value::String(file_id.to_string()));
    }
    if let Some(detail) = part.get("detail").cloned() {
        map.insert("detail".to_string(), detail);
    }
    Value::Object(map)
}

fn normalize_image_url_part(part: &Value) -> Value {
    json!({
        "type": "input_image",
        "image_url": response_image_url_value(part).unwrap_or_default(),
        "detail": part
            .get("detail")
            .cloned()
            .unwrap_or(Value::String("auto".to_string()))
    })
}

fn response_image_url_value(part: &Value) -> Option<String> {
    part.get("image_url")
        .and_then(|value| {
            value.as_str().map(str::to_string).or_else(|| {
                value
                    .get("url")
                    .and_then(|inner| inner.as_str())
                    .map(str::to_string)
            })
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn image_part_locator(part: &Value) -> &str {
    part.get("image_url")
        .and_then(|value| value.get("url"))
        .and_then(|value| value.as_str())
        .or_else(|| part.get("image_url").and_then(|value| value.as_str()))
        .or_else(|| part.get("file_id").and_then(|value| value.as_str()))
        .unwrap_or("")
}

fn text_or_json_string(value: &Value, keys: &[&str]) -> String {
    if value.is_null() {
        return String::new();
    }

    text_value_or_json(value, keys).unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        append_input_items, assistant_visible_text, content_parts_to_text,
        convert_parts_to_response_input, response_content_has_image_part, to_message_item,
        to_message_item_with_reasoning,
    };

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

    #[test]
    fn assistant_messages_do_not_emit_reasoning_content_items() {
        let item = to_message_item_with_reasoning(
            "assistant",
            &Value::String("Visible answer".to_string()),
            Some("Internal chain of thought"),
            false,
        );

        assert_eq!(
            item,
            json!({
                "role": "assistant",
                "type": "message",
                "content": [{"type": "output_text", "text": "Visible answer"}]
            })
        );
    }

    #[test]
    fn to_message_item_normalizes_legacy_content_parts_for_responses() {
        let content = json!([
            {"type": "text", "text": "runtime guidance"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,Zm9v"}, "detail": "high"}
        ]);

        let item = to_message_item("system", &content, false);

        assert_eq!(
            item,
            json!({
                "role": "system",
                "type": "message",
                "content": [
                    {"type": "input_text", "text": "runtime guidance"},
                    {"type": "input_image", "image_url": "data:image/png;base64,Zm9v", "detail": "high"}
                ]
            })
        );
    }

    #[test]
    fn assistant_visible_text_uses_reasoning_only_as_fallback() {
        assert_eq!(assistant_visible_text("answer", Some("trace")), "answer");
        assert_eq!(assistant_visible_text("   ", Some("trace")), "trace");
        assert_eq!(assistant_visible_text("   ", Some("   ")), "");
    }

    #[test]
    fn append_input_items_force_text_normalizes_appended_messages() {
        let appended = json!({
            "role": "user",
            "type": "message",
            "content": [
                {"type": "input_text", "text": "runtime guidance"},
                {"type": "input_image", "image_url": "data:image/png;base64,Zm9v"}
            ]
        });

        let payload = append_input_items(&Value::String("current".to_string()), &[appended], true);
        let items = payload.as_array().expect("input should be a message list");
        let appended_content = items[1]
            .get("content")
            .and_then(|value| value.as_str())
            .expect("force text should convert appended message content to string");

        assert!(appended_content.contains("runtime guidance"));
        assert!(appended_content.contains("[image:data:image/png;base64,Zm9v]"));
    }

    #[test]
    fn response_content_has_image_part_detects_input_images() {
        let content = json!([
            {"type": "input_text", "text": "hello"},
            {"type": "input_image", "image_url": "data:image/png;base64,Zm9v"}
        ]);

        assert!(response_content_has_image_part(&content));
    }
}
