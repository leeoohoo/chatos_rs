use serde_json::Value;

pub fn to_text_and_structured_result(result: &Value) -> (String, Option<Value>) {
    let structured_result = result.get("_structured_result").cloned();
    let raw = if let Some(text) = result.as_str() {
        text.to_string()
    } else if let Some(content) = result.get("content").and_then(|value| value.as_array()) {
        let mut extracted: Option<String> = None;
        for item in content {
            if item.get("type").and_then(|value| value.as_str()) != Some("text") {
                continue;
            }
            if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                extracted = Some(text.to_string());
                break;
            }
            if let Some(value) = item.get("value").and_then(|value| value.as_str()) {
                extracted = Some(value.to_string());
                break;
            }
        }
        extracted.unwrap_or_else(|| result.to_string())
    } else if let Some(text) = result.get("text").and_then(|value| value.as_str()) {
        text.to_string()
    } else if let Some(value) = result.get("value").and_then(|value| value.as_str()) {
        value.to_string()
    } else {
        result.to_string()
    };

    (
        truncate_tool_text(raw.as_str(), tool_result_text_max_chars()),
        structured_result,
    )
}

pub fn inject_agent_builder_args(args: Value, caller_model: Option<&str>) -> Value {
    let Some(model_name) = caller_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return args;
    };

    let mut obj = match args {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        _ => return args,
    };

    obj.entry("caller_model".to_string())
        .or_insert_with(|| Value::String(model_name.to_string()));

    Value::Object(obj)
}

fn tool_result_text_max_chars() -> usize {
    std::env::var("MCP_TOOL_RESULT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(16_000)
}

pub(crate) fn truncate_tool_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }

    let marker = format!("\n...[truncated {} chars]...\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return text.chars().take(max_chars).collect();
    }

    let head_chars = ((max_chars - marker_chars) * 3 / 5).max(1);
    let tail_chars = (max_chars - marker_chars).saturating_sub(head_chars);
    let head: String = text.chars().take(head_chars).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}{}", head, marker, tail)
}
