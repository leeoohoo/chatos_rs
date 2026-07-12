// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

#[cfg(test)]
use crate::core::messages::{optional_text_has_content, select_preferred_text, text_has_content};
#[cfg(test)]
use crate::core::tool_call::tool_calls_value_has_items;

#[cfg(test)]
use super::request_transport::truncate_log;

pub(crate) const TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE: &str = "task_runner_async_plan";

pub(crate) fn build_ai_client_success_payload(
    content: String,
    reasoning: Option<String>,
    finish_reason: Option<String>,
    iteration: i64,
) -> Value {
    serde_json::json!({
        "success": true,
        "content": content,
        "reasoning": reasoning,
        "tool_calls": Value::Null,
        "finish_reason": finish_reason,
        "iteration": iteration
    })
}

pub(crate) fn attach_ai_client_success_extra(payload: Value, extra: Value) -> Value {
    let mut base = match payload {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };
    if let Value::Object(extra_map) = extra {
        for (key, value) in extra_map {
            base.insert(key, value);
        }
    }
    Value::Object(base)
}

#[cfg(test)]
pub(crate) fn completion_failed_error(
    finish_reason: Option<&str>,
    content: &str,
    reasoning: Option<&str>,
    provider_error: Option<&Value>,
) -> Option<String> {
    let reason = finish_reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let normalized = reason.to_ascii_lowercase();
    if normalized != "failed" && normalized != "error" {
        return None;
    }

    let mut segments = vec![format!("finish_reason={}", reason)];

    if let Some(error_preview) = provider_error
        .and_then(build_provider_error_preview)
        .filter(|value| !value.trim().is_empty())
    {
        segments.push(format!("provider_error={}", error_preview));
    }

    if let Some(preferred_text) = select_preferred_text(content, reasoning) {
        let preview_key = if text_has_content(content) {
            "content_preview"
        } else {
            "reasoning_preview"
        };
        segments.push(format!(
            "{}={}",
            preview_key,
            truncate_log(preferred_text, 300)
        ));
        return Some(format!("ai response failed: {}", segments.join("; ")));
    }

    Some(format!("ai response failed: {}", segments.join("; ")))
}

#[cfg(test)]
pub(crate) fn terminal_empty_response_error(
    finish_reason: Option<&str>,
    content: &str,
    reasoning: Option<&str>,
    tool_calls: Option<&Value>,
    provider_error: Option<&Value>,
) -> Option<String> {
    if is_non_terminal_response_status(finish_reason) {
        return None;
    }

    if text_has_content(content)
        || optional_text_has_content(reasoning)
        || tool_calls_value_has_items(tool_calls)
    {
        return None;
    }

    let reason = finish_reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let mut segments = vec![format!("finish_reason={}", reason)];

    if let Some(error_preview) = provider_error
        .and_then(build_provider_error_preview)
        .filter(|value| !value.trim().is_empty())
    {
        segments.push(format!("provider_error={}", error_preview));
    }

    Some(format!(
        "ai response invalid: terminal empty response; {}",
        segments.join("; ")
    ))
}

#[cfg(test)]
pub(crate) fn should_persist_assistant_message(
    content: &str,
    reasoning: Option<&str>,
    tool_calls: Option<&Value>,
    _response_status: Option<&str>,
) -> bool {
    let has_content = text_has_content(content);
    let has_reasoning = optional_text_has_content(reasoning);
    let has_tool_calls = tool_calls_value_has_items(tool_calls);
    if has_content || has_reasoning || has_tool_calls {
        return true;
    }
    false
}

#[cfg(test)]
pub(crate) fn is_task_runner_async_plan_message_mode(message_mode: Option<&str>) -> bool {
    matches!(
        message_mode
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        Some(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE)
    )
}

pub(crate) fn normalize_task_runner_async_plan_metadata(metadata: Option<Value>) -> Option<Value> {
    normalize_task_runner_async_metadata(metadata, "plan_summary")
}

pub(crate) fn normalize_task_runner_async_tool_call_metadata(
    metadata: Option<Value>,
) -> Option<Value> {
    normalize_task_runner_async_metadata(metadata, "tool_call")
}

fn normalize_task_runner_async_metadata(
    metadata: Option<Value>,
    message_kind: &str,
) -> Option<Value> {
    let mut root = match metadata {
        Some(Value::Object(map)) => map,
        Some(_) | None => serde_json::Map::new(),
    };

    let task_runner_async = root
        .entry("task_runner_async".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Value::Object(task_runner_async_map) = task_runner_async else {
        root.insert(
            "task_runner_async".to_string(),
            serde_json::json!({
                "mode": "contact_async",
                "message_kind": message_kind
            }),
        );
        return Some(Value::Object(root));
    };

    task_runner_async_map.insert(
        "mode".to_string(),
        Value::String("contact_async".to_string()),
    );
    task_runner_async_map.insert(
        "message_kind".to_string(),
        Value::String(message_kind.to_string()),
    );
    Some(Value::Object(root))
}

pub(crate) fn build_assistant_message_metadata(
    tool_calls: Option<&Value>,
    response_id: Option<&str>,
    turn_id: Option<&str>,
    response_status: Option<&str>,
    extra_metadata: Option<&Value>,
) -> Option<Value> {
    let mut map = serde_json::Map::new();

    if let Some(turn) = turn_id.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(
            "conversation_turn_id".to_string(),
            Value::String(turn.to_string()),
        );
    }
    if let Some(id) = response_id.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert("response_id".to_string(), Value::String(id.to_string()));
    }
    if let Some(status) = response_status
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        map.insert(
            "response_status".to_string(),
            Value::String(status.to_string()),
        );
    }
    if let Some(tool_calls) = tool_calls {
        map.insert("toolCalls".to_string(), tool_calls.clone());
    }
    if let Some(Value::Object(extra)) = extra_metadata {
        for (key, value) in extra {
            map.insert(key.clone(), value.clone());
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

#[cfg(test)]
fn is_non_terminal_response_status(status: Option<&str>) -> bool {
    let normalized = status
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    matches!(
        normalized.as_deref(),
        Some("in_progress") | Some("queued") | Some("pending") | Some("incomplete")
    )
}

#[cfg(test)]
fn build_provider_error_preview(provider_error: &Value) -> Option<String> {
    if provider_error.is_null() {
        return None;
    }

    if let Some(object) = provider_error.as_object() {
        let mut parts = Vec::new();
        if let Some(code) = object.get("code").and_then(|value| value.as_str()) {
            if text_has_content(code) {
                parts.push(format!("code={}", code.trim()));
            }
        }
        if let Some(kind) = object.get("type").and_then(|value| value.as_str()) {
            if text_has_content(kind) {
                parts.push(format!("type={}", kind.trim()));
            }
        }
        if let Some(message) = object.get("message").and_then(|value| value.as_str()) {
            if text_has_content(message) {
                parts.push(format!("message={}", truncate_log(message.trim(), 300)));
            }
        }
        if let Some(param) = object.get("param").and_then(|value| value.as_str()) {
            if text_has_content(param) {
                parts.push(format!("param={}", param.trim()));
            }
        }

        if !parts.is_empty() {
            return Some(parts.join(", "));
        }
    }

    let raw = provider_error.to_string();
    if !text_has_content(raw.as_str()) {
        None
    } else {
        Some(truncate_log(raw.trim(), 300))
    }
}
