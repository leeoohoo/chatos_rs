// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use super::*;

const SECRET_VALUE_MASK: &str = "******";

pub(in crate::ask_user_prompt_service) fn prompt_to_decision(
    prompt: AskUserPromptRecord,
) -> AskUserDecision {
    let response = prompt
        .response
        .unwrap_or_else(|| AskUserResponseSubmission {
            status: status_label(prompt.status).to_string(),
            values: None,
            selection: None,
            reason: None,
        });
    AskUserDecision {
        status: response.status.clone(),
        response,
    }
}

pub(in crate::ask_user_prompt_service) fn prompt_event_payload(
    prompt: &AskUserPromptRecord,
) -> Value {
    let secret_keys = secret_field_keys(&prompt.payload);
    json!({
        "prompt_id": prompt.id,
        "task_id": prompt.task_id,
        "run_id": prompt.run_id,
        "kind": prompt.kind,
        "title": prompt.title,
        "message": prompt.message,
        "status": status_label(prompt.status),
        "allow_cancel": prompt.allow_cancel,
        "timeout_ms": prompt.timeout_ms,
        "payload": redacted_prompt_payload(prompt.payload.clone()),
        "response": redacted_prompt_response(prompt.response.clone(), &secret_keys),
        "expires_at": prompt.expires_at,
    })
}

pub(in crate::ask_user_prompt_service) fn redacted_prompt_response(
    mut response: Option<AskUserResponseSubmission>,
    secret_keys: &BTreeSet<String>,
) -> Option<AskUserResponseSubmission> {
    if let Some(response) = response.as_mut() {
        if let Some(values) = response.values.as_mut() {
            redact_secret_values(values, secret_keys);
        }
    }
    response
}

pub(in crate::ask_user_prompt_service) fn redacted_prompt_payload(mut payload: Value) -> Value {
    redact_secret_field_defaults(&mut payload);
    payload
}

pub(in crate::ask_user_prompt_service) fn secret_field_keys(payload: &Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let Some(fields) = payload.get("fields").and_then(Value::as_array) else {
        return keys;
    };
    for field in fields {
        if field.get("secret").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        for key_name in ["key", "name", "id"] {
            if let Some(key) = field.get(key_name).and_then(Value::as_str) {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    keys.insert(trimmed.to_string());
                }
            }
        }
    }
    keys
}

fn redact_secret_field_defaults(payload: &mut Value) {
    let Some(fields) = payload.get_mut("fields").and_then(Value::as_array_mut) else {
        return;
    };
    for field in fields {
        if field.get("secret").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(map) = field.as_object_mut() else {
            continue;
        };
        let should_mask_default = map.get("default").is_some_and(has_sensitive_value);
        if should_mask_default {
            map.insert(
                "default".to_string(),
                Value::String(SECRET_VALUE_MASK.to_string()),
            );
        }
    }
}

fn redact_secret_values(value: &mut Value, secret_keys: &BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for key in secret_keys {
                if let Some(item) = map.get_mut(key) {
                    redact_value(item);
                }
            }
            for item in map.values_mut() {
                redact_secret_values(item, secret_keys);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_secret_values(item, secret_keys);
            }
        }
        _ => {}
    }
}

fn redact_value(value: &mut Value) {
    if has_sensitive_value(value) {
        *value = Value::String(SECRET_VALUE_MASK.to_string());
    }
}

fn has_sensitive_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

pub(in crate::ask_user_prompt_service) fn status_label(
    status: AskUserPromptStatus,
) -> &'static str {
    match status {
        AskUserPromptStatus::Pending => "pending",
        AskUserPromptStatus::Submitted => "submitted",
        AskUserPromptStatus::Cancelled => "cancelled",
        AskUserPromptStatus::TimedOut => "timed_out",
        AskUserPromptStatus::Failed => "failed",
    }
}

pub(in crate::ask_user_prompt_service) fn normalized_optional(
    value: Option<String>,
) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_event_payload_redacts_secret_response_values() {
        let prompt = AskUserPromptRecord {
            id: "prompt_1".to_string(),
            task_id: Some("task_1".to_string()),
            run_id: Some("run_1".to_string()),
            conversation_id: "conversation_1".to_string(),
            conversation_turn_id: "turn_1".to_string(),
            tool_call_id: None,
            kind: "mixed".to_string(),
            title: "Deploy".to_string(),
            message: "Need values".to_string(),
            allow_cancel: true,
            timeout_ms: 1000,
            payload: json!({
                "fields": [
                    {"key": "host", "label": "Host", "secret": false},
                    {"key": "password", "label": "Password", "secret": true},
                    {"key": "token", "label": "Token", "secret": true, "default": "seed-token"}
                ],
                "choice": {"options": []}
            }),
            response: Some(AskUserResponseSubmission {
                status: "submitted".to_string(),
                values: Some(json!({
                    "host": "example.com",
                    "password": "super-secret",
                    "nested": {"token": "nested-secret"}
                })),
                selection: Some(json!("proceed")),
                reason: None,
            }),
            status: AskUserPromptStatus::Submitted,
            created_at: "2026-07-01T00:00:00Z".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
            expires_at: None,
        };

        let payload = prompt_event_payload(&prompt);

        assert_eq!(
            payload["payload"]["fields"][2]["default"],
            SECRET_VALUE_MASK
        );
        assert_eq!(payload["response"]["values"]["host"], "example.com");
        assert_eq!(payload["response"]["values"]["password"], SECRET_VALUE_MASK);
        assert_eq!(
            payload["response"]["values"]["nested"]["token"],
            SECRET_VALUE_MASK
        );
        assert_eq!(payload["response"]["selection"], "proceed");
    }
}
