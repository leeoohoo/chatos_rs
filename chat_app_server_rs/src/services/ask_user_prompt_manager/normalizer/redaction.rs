// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::support::SECRET_MASK;
use super::{AskUserPromptPayload, AskUserPromptResponseSubmission};

pub fn redact_prompt_payload(payload: &AskUserPromptPayload) -> Value {
    let mut prompt_value = serde_json::to_value(payload).unwrap_or_else(|_| json!({}));

    if let Some(fields) = prompt_value
        .get_mut("payload")
        .and_then(|value| value.get_mut("fields"))
        .and_then(Value::as_array_mut)
    {
        for field in fields {
            let secret = field
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if secret {
                if let Some(default_value) = field.get_mut("default") {
                    *default_value = Value::String(SECRET_MASK.to_string());
                }
                if let Some(default_value) = field.get_mut("default_value") {
                    *default_value = Value::String(SECRET_MASK.to_string());
                }
            }
        }
    }

    prompt_value
}

pub fn redact_response_for_store(
    response: &AskUserPromptResponseSubmission,
    payload: &AskUserPromptPayload,
) -> Value {
    let mut out = serde_json::to_value(response).unwrap_or_else(|_| json!({}));
    let Some(secret_keys) = secret_field_keys(payload) else {
        return out;
    };

    let Some(values_obj) = out.get_mut("values").and_then(Value::as_object_mut) else {
        return out;
    };

    for key in secret_keys {
        if values_obj.contains_key(key.as_str()) {
            values_obj.insert(key, Value::String(SECRET_MASK.to_string()));
        }
    }

    out
}

fn secret_field_keys(payload: &AskUserPromptPayload) -> Option<Vec<String>> {
    let fields = payload.payload.get("fields").and_then(Value::as_array)?;

    let keys: Vec<String> = fields
        .iter()
        .filter_map(|field| {
            if !field
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return None;
            }
            let key = field
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if key.is_empty() {
                None
            } else {
                Some(key.to_string())
            }
        })
        .collect();

    if keys.is_empty() {
        None
    } else {
        Some(keys)
    }
}
