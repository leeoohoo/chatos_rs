// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::{json, Value};

use crate::relay::{RelayRequest, RelayResponse};

pub(super) fn decode_request(response_type: &str, value: Value) -> Result<RelayRequest, Value> {
    serde_json::from_value::<RelayRequest>(value)
        .map_err(|error| plugin_error_response(response_type, "", 400, error.to_string()))
}

pub(super) fn plugin_response(
    response_type: &str,
    request: &RelayRequest,
    result: Result<Value, (u16, String)>,
) -> Value {
    match result {
        Ok(body) => RelayResponse {
            message_type: response_type.to_string(),
            request_id: request.request_id.clone(),
            status: 200,
            headers: BTreeMap::new(),
            body,
        }
        .into_value(),
        Err((status, message)) => {
            plugin_error_response(response_type, request.request_id.as_str(), status, message)
        }
    }
}

pub(super) fn validate_prepared_release(
    actual_release_id: &str,
    actual_artifact_sha256: &str,
    expected_release_id: &str,
    expected_artifact_sha256: &str,
) -> Result<(), (u16, String)> {
    if actual_release_id != expected_release_id
        || actual_artifact_sha256 != expected_artifact_sha256
    {
        return Err((
            409,
            "Plugin prepare snapshot does not match the active immutable Release".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn required_envelope_text(
    value: Option<&str>,
    field: &str,
) -> Result<String, (u16, String)> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| (400, format!("{field} is required")))
}

pub(super) fn required_body_text(body: &Value, field: &str) -> Result<String, (u16, String)> {
    body.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| (400, format!("{field} is required")))
}

pub(super) fn optional_body_text(
    body: &Value,
    field: &str,
) -> Result<Option<String>, (u16, String)> {
    let Some(value) = body.get(field) else {
        return Ok(None);
    };
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| (400, format!("{field} must be a non-empty string")))
}

pub(super) fn optional_body_bool(body: &Value, field: &str) -> Result<Option<bool>, (u16, String)> {
    let Some(value) = body.get(field) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| (400, format!("{field} must be a boolean")))
}

pub(super) fn required_sha256(body: &Value, field: &str) -> Result<String, (u16, String)> {
    let value = required_body_text(body, field)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err((400, format!("{field} must be a lower-case SHA-256")));
    }
    Ok(value)
}

pub(super) fn required_body_text_array(
    body: &Value,
    field: &str,
    max_items: usize,
) -> Result<Vec<String>, (u16, String)> {
    let values = body
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| (400, format!("{field} must be an array")))?;
    if values.is_empty() || values.len() > max_items {
        return Err((
            400,
            format!("{field} must contain between 1 and {max_items} items"),
        ));
    }
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| (400, format!("{field} contains an invalid item")))?;
            if !seen.insert(value.to_string()) {
                return Err((400, format!("{field} contains a duplicate item")));
            }
            Ok(value.to_string())
        })
        .collect()
}

pub(super) fn optional_body_text_set(
    body: &Value,
    field: &str,
    max_items: usize,
) -> Result<BTreeSet<String>, (u16, String)> {
    let Some(values) = body.get(field) else {
        return Ok(BTreeSet::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| (400, format!("{field} must be an array")))?;
    if values.len() > max_items {
        return Err((
            400,
            format!("{field} must contain at most {max_items} items"),
        ));
    }
    let mut result = BTreeSet::new();
    for value in values {
        let value = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| (400, format!("{field} contains an invalid item")))?;
        if !result.insert(value.to_string()) {
            return Err((400, format!("{field} contains a duplicate item")));
        }
    }
    Ok(result)
}

fn plugin_error_response(
    message_type: &str,
    request_id: &str,
    status: u16,
    message: String,
) -> Value {
    RelayResponse {
        message_type: message_type.to_string(),
        request_id: request_id.to_string(),
        status,
        headers: BTreeMap::new(),
        body: json!({ "error": message }),
    }
    .into_value()
}

pub(super) fn internal_error(error: anyhow::Error) -> (u16, String) {
    (500, error.to_string())
}
