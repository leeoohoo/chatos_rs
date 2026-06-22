use serde_json::Value;

use super::PROCESS_WAIT_MAX_TIMEOUT_MS;

pub fn resolve_wait_timeout_ms(args: &Value) -> u64 {
    args.get("timeout_ms")
        .and_then(Value::as_u64)
        .or_else(|| {
            args.get("timeout")
                .and_then(Value::as_u64)
                .map(|seconds| seconds.saturating_mul(1_000))
        })
        .unwrap_or(30_000)
        .clamp(1_000, PROCESS_WAIT_MAX_TIMEOUT_MS)
}

pub fn coerce_process_identifier(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(raw)) => Some(raw.to_string()),
        _ => None,
    }
}

pub(super) fn coerce_process_data(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(raw)) => Some(raw.to_string()),
        Some(Value::Number(raw)) => Some(raw.to_string()),
        Some(Value::Bool(raw)) => Some(raw.to_string()),
        Some(Value::Null) => Some(String::new()),
        Some(other) => Some(other.to_string()),
        None => None,
    }
}

pub(super) fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} is required"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(trimmed.to_string())
    }
}
