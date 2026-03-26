use serde_json::Value;

use crate::services::memory_server_client::MemoryAgentSkillDto;

pub(super) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub(super) fn required_string(args: &Value, key: &str) -> Result<String, String> {
    optional_string(args, key).ok_or_else(|| format!("missing required field: {}", key))
}

pub(super) fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn optional_string_array(args: &Value, key: &str) -> Option<Vec<String>> {
    let values = args.get(key)?.as_array()?;
    let mut out = Vec::new();
    for value in values {
        let Some(item) = value.as_str() else {
            continue;
        };
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub(super) fn optional_skill_array(args: &Value, key: &str) -> Option<Vec<MemoryAgentSkillDto>> {
    let values = args.get(key)?.as_array()?;
    let mut out = Vec::new();
    for item in values {
        let Some(object) = item.as_object() else {
            continue;
        };
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let content = object
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let (Some(id), Some(name), Some(content)) = (id, name, content) else {
            continue;
        };
        out.push(MemoryAgentSkillDto { id, name, content });
    }
    Some(out)
}

pub(super) fn optional_object_value(args: &Value, key: &str) -> Option<Value> {
    let value = args.get(key)?;
    if !value.is_object() {
        return None;
    }
    Some(value.clone())
}

pub(super) fn normalize_tool_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((_, suffix)) = trimmed.rsplit_once("__") {
        return suffix.trim().to_string();
    }
    trimmed.to_string()
}

pub(super) fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(max_chars).collect();
    out.push_str("...");
    out
}
