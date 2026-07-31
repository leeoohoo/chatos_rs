// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};

pub(super) fn ensure_exact_arguments(arguments: &Value, allowed: &[&str]) -> Result<()> {
    let object = arguments
        .as_object()
        .context("Excel Live Control arguments must be an object")?;
    if object.len() != allowed.len()
        || object
            .keys()
            .any(|key| !allowed.iter().any(|allowed| key == allowed))
    {
        bail!("Excel Live Control arguments contain unknown or missing fields");
    }
    Ok(())
}

pub(super) fn required_text<'a>(
    arguments: &'a Value,
    field: &str,
    max_characters: usize,
) -> Result<&'a str> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{field} is required"))?;
    validate_bounded_text(value, field, max_characters)?;
    Ok(value)
}

pub(super) fn required_bounded_text<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    max_characters: usize,
) -> Result<&'a str> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Excel automation response is missing {field}"))?;
    validate_bounded_text(value, field, max_characters)?;
    Ok(value)
}

pub(super) fn optional_bounded_text(
    value: Option<&Value>,
    field: &str,
    max_characters: usize,
) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => {
            validate_bounded_text(value, field, max_characters)?;
            Ok(Some(value.clone()))
        }
        _ => bail!("Excel automation response has an invalid {field}"),
    }
}

pub(super) fn validate_bounded_text(value: &str, field: &str, max_characters: usize) -> Result<()> {
    if value.chars().count() > max_characters {
        bail!("Excel automation {field} exceeds the bounded text limit");
    }
    if value.chars().any(char::is_control) {
        bail!("Excel automation {field} contains a control character");
    }
    Ok(())
}

pub(super) fn required_bool(object: &Map<String, Value>, field: &str) -> Result<bool> {
    object
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("Excel automation response is missing boolean {field}"))
}

pub(super) fn required_usize(
    object: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<usize> {
    let value = object
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow!("Excel automation response is missing integer {field}"))?;
    if value > maximum {
        bail!("Excel automation {field} exceeds the bounded limit");
    }
    Ok(value)
}
