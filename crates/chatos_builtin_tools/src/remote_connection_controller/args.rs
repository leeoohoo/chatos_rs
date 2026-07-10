// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(super) fn required_string(args: &Value, field: &str) -> Result<String, String> {
    args.get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{field} is required"))
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

pub(super) fn optional_trimmed_string(args: &Value, field: &str) -> Option<String> {
    args.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn optional_u64(args: &Value, field: &str) -> Option<u64> {
    args.get(field).and_then(Value::as_u64)
}

pub(super) fn optional_usize(args: &Value, field: &str) -> Option<usize> {
    args.get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

pub(super) fn optional_bool(args: &Value, field: &str) -> bool {
    args.get(field).and_then(Value::as_bool).unwrap_or(false)
}

pub(super) fn optional_bool_with_default(args: &Value, field: &str, default: bool) -> bool {
    args.get(field).and_then(Value::as_bool).unwrap_or(default)
}

pub(super) fn optional_encoding(
    args: &Value,
    field: &str,
    default: &str,
) -> Result<String, String> {
    let encoding = optional_trimmed_string(args, field).unwrap_or_else(|| default.to_string());
    match encoding.as_str() {
        "text" | "base64" => Ok(encoding),
        _ => Err(format!("{field} must be one of: text, base64")),
    }
}
