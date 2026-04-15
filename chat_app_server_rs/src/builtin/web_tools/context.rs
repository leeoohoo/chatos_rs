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

pub(super) fn optional_usize(args: &Value, field: &str) -> Option<usize> {
    args.get(field)
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
}

pub(super) fn required_string_array(args: &Value, field: &str) -> Result<Vec<String>, String> {
    let items = args
        .get(field)
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("{field} is required"))?;

    let mut out = Vec::new();
    for item in items {
        let Some(text) = item.as_str() else {
            return Err(format!("{field} must contain strings"));
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(trimmed.to_string());
    }
    if out.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(out)
}
