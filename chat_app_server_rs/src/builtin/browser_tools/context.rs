use serde_json::Value;

pub(super) fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{field} is required"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(trimmed.to_string())
}

pub(super) fn optional_trimmed_string(args: &Value, field: &str) -> Option<String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

pub(super) fn optional_bool(args: &Value, field: &str) -> bool {
    args.get(field)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(super) fn conversation_key(conversation_id: Option<&str>) -> String {
    conversation_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default")
        .to_string()
}
