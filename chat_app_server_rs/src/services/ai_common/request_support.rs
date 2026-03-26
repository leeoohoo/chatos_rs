use std::future::Future;

use reqwest::RequestBuilder;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::utils::abort_registry;
use crate::utils::attachments::{self, Attachment};

pub(crate) fn normalize_turn_id(turn_id: Option<&str>) -> Option<String> {
    turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

pub(crate) fn build_user_message_metadata(
    attachments_list: &[Attachment],
    turn_id: Option<&str>,
) -> Option<Value> {
    let sanitized = attachments::sanitize_attachments_for_db(attachments_list);

    if sanitized.is_empty() && turn_id.is_none() {
        return None;
    }

    let mut map = serde_json::Map::new();
    if !sanitized.is_empty() {
        map.insert("attachments".to_string(), Value::Array(sanitized));
    }
    if let Some(turn) = turn_id {
        map.insert(
            "conversation_turn_id".to_string(),
            Value::String(turn.to_string()),
        );
    }

    Some(Value::Object(map))
}

pub(crate) async fn build_user_content_parts(
    model: &str,
    user_message: &str,
    attachments_list: &[Attachment],
    supports_images: Option<bool>,
) -> Value {
    let content_parts =
        attachments::build_content_parts_async(user_message, attachments_list).await;
    attachments::adapt_parts_for_model(model, &content_parts, supports_images)
}

pub(crate) fn normalize_reasoning_effort(
    provider: Option<&str>,
    level: Option<&str>,
) -> Option<String> {
    let provider = provider.unwrap_or("gpt");
    crate::utils::model_config::normalize_thinking_level(provider, level).unwrap_or_default()
}

pub(crate) fn truncate_log(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut out = value[..max_len].to_string();
    out.push_str("...[truncated]");
    out
}

pub(crate) fn build_abort_token(session_id: Option<&str>) -> Option<CancellationToken> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let token = CancellationToken::new();
    abort_registry::set_controller(session_id, token.clone());
    Some(token)
}

pub(crate) fn build_bearer_post_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    force_identity_encoding: bool,
) -> RequestBuilder {
    let mut req = client.post(url).bearer_auth(api_key);
    if force_identity_encoding {
        req = req
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::CONNECTION, "close")
            .version(reqwest::Version::HTTP_11);
    }
    req
}

pub(crate) fn validate_request_payload_size(payload: &Value, env_key: &str) -> Result<(), String> {
    let bytes = serde_json::to_vec(payload).map_err(|err| err.to_string())?;
    let max_bytes = request_payload_max_bytes(env_key);
    if bytes.len() > max_bytes {
        return Err(format!(
            "request body too large (precheck): payload_bytes={}, limit_bytes={}",
            bytes.len(),
            max_bytes
        ));
    }
    Ok(())
}

fn request_payload_max_bytes(env_key: &str) -> usize {
    std::env::var(env_key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1_500_000)
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

    if !content.trim().is_empty() {
        segments.push(format!("content_preview={}", truncate_log(content, 300)));
        return Some(format!("ai response failed: {}", segments.join("; ")));
    }

    if let Some(reasoning) = reasoning {
        if !reasoning.trim().is_empty() {
            segments.push(format!(
                "reasoning_preview={}",
                truncate_log(reasoning, 300)
            ));
            return Some(format!("ai response failed: {}", segments.join("; ")));
        }
    }

    Some(format!("ai response failed: {}", segments.join("; ")))
}

fn build_provider_error_preview(provider_error: &Value) -> Option<String> {
    if provider_error.is_null() {
        return None;
    }

    if let Some(object) = provider_error.as_object() {
        let mut parts = Vec::new();
        if let Some(code) = object.get("code").and_then(|v| v.as_str()) {
            if !code.trim().is_empty() {
                parts.push(format!("code={}", code.trim()));
            }
        }
        if let Some(kind) = object.get("type").and_then(|v| v.as_str()) {
            if !kind.trim().is_empty() {
                parts.push(format!("type={}", kind.trim()));
            }
        }
        if let Some(message) = object.get("message").and_then(|v| v.as_str()) {
            if !message.trim().is_empty() {
                parts.push(format!("message={}", truncate_log(message.trim(), 300)));
            }
        }
        if let Some(param) = object.get("param").and_then(|v| v.as_str()) {
            if !param.trim().is_empty() {
                parts.push(format!("param={}", param.trim()));
            }
        }

        if !parts.is_empty() {
            return Some(parts.join(", "));
        }
    }

    let raw = provider_error.to_string();
    if raw.trim().is_empty() {
        None
    } else {
        Some(truncate_log(raw.trim(), 300))
    }
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
    if let Some(status) = response_status.map(str::trim).filter(|value| !value.is_empty()) {
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

pub(crate) async fn await_with_optional_abort<F, T, E>(
    future: F,
    token: Option<CancellationToken>,
) -> Result<T, String>
where
    F: Future<Output = Result<T, E>>,
    E: ToString,
{
    if let Some(token) = token {
        tokio::select! {
            _ = token.cancelled() => Err("aborted".to_string()),
            value = future => value.map_err(|err| err.to_string()),
        }
    } else {
        future.await.map_err(|err| err.to_string())
    }
}
