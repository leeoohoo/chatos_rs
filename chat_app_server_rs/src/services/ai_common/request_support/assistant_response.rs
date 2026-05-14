use std::future::Future;

use serde_json::Value;
use tracing::{error, info};

use crate::core::messages::{
    object_string_alias, optional_text_has_content, select_preferred_text, text_has_content,
};
use crate::core::tool_call::tool_calls_value_has_items;

use super::request_transport::truncate_log;

pub(crate) struct AssistantResponsePersistenceRequest {
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub persist_messages: bool,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub response_id: Option<String>,
    pub response_status: Option<String>,
}

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

pub(crate) fn build_assistant_message_metadata(
    tool_calls: Option<&Value>,
    response_id: Option<&str>,
    turn_id: Option<&str>,
    response_status: Option<&str>,
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

    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

pub(crate) async fn persist_assistant_response_with_policy<F, Fut>(
    request: AssistantResponsePersistenceRequest,
    should_persist: bool,
    log_prefix: &str,
    skip_log_label: Option<&str>,
    save_assistant_message: F,
) where
    F: FnOnce(AssistantResponsePersistenceRequest) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    if !request.persist_messages {
        return;
    }

    if should_persist {
        if let Some(session_id) = request.session_id.clone() {
            if let Err(err) = save_assistant_message(request).await {
                error!(
                    "{} save assistant message failed: session_id={}, detail={}",
                    log_prefix, session_id, err
                );
            }
        }
        return;
    }

    if let Some(skip_log_label) = skip_log_label {
        info!(
            "{} skip assistant message persistence due to {}: session_id={}, turn_id={}, response_id={}, finish_reason={}",
            log_prefix,
            skip_log_label,
            request.session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            request.turn_id.clone().unwrap_or_else(|| "n/a".to_string()),
            request.response_id.as_deref().unwrap_or("none"),
            request.response_status.as_deref().unwrap_or("none")
        );
    }
}

pub(crate) fn extract_response_status_from_metadata(metadata: &Value) -> Option<&str> {
    object_string_alias(
        metadata,
        &[
            "response_status",
            "responseStatus",
            "finish_reason",
            "finishReason",
            "status",
        ],
    )
}

pub(crate) fn extract_response_id_from_metadata(metadata: &Value) -> Option<&str> {
    object_string_alias(metadata, &["response_id", "responseId"])
}

pub(crate) fn is_non_terminal_response_status(status: Option<&str>) -> bool {
    let normalized = status
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    matches!(
        normalized.as_deref(),
        Some("in_progress") | Some("queued") | Some("pending") | Some("incomplete")
    )
}

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
