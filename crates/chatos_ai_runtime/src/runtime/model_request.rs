// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::request::{AiRequestHandler, AiRequestOptions, AiResponse, StreamCallbacks};
use crate::traits::{ModelRequest, RuntimeCallbacks};

use super::input_items::attach_runtime_debug;
use super::options::AiRuntimeOptions;

const PROVIDER_ERROR_DETAIL_MAX_CHARS: usize = 2_000;

pub(super) async fn dispatch_model_request(
    request_handler: &AiRequestHandler,
    request: &ModelRequest,
    options: &AiRuntimeOptions,
    iteration: usize,
    iteration_reason: &str,
    input_item_count: usize,
    input_bytes: usize,
    tool_count: usize,
    stream_output: bool,
) -> Result<AiResponse, String> {
    info!(
        conversation_id = options.conversation_id.as_deref().unwrap_or(""),
        conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
        iteration,
        reason = iteration_reason,
        model = request.model.as_str(),
        provider = request.provider.as_str(),
        supports_responses = request.supports_responses,
        input_item_count,
        input_bytes,
        tool_count,
        "ai runtime dispatching model request"
    );
    let request_debug = json!({
        "conversation_id": options.conversation_id.clone(),
        "conversation_turn_id": options.conversation_turn_id.clone(),
        "iteration": iteration,
        "reason": iteration_reason,
        "input_item_count": input_item_count,
        "input_bytes": input_bytes,
        "tool_count": tool_count,
        "supports_responses": request.supports_responses,
    });
    if let Some(callback) = &options.callbacks.on_before_model_input {
        callback(request.input.clone());
    }
    let on_before_send_model_request =
        build_before_send_model_request_callback(&options.callbacks, request_debug);

    let started_at = Instant::now();
    let result = request_handler
        .handle_request_with_options(
            request.base_url.as_str(),
            request.api_key.as_str(),
            request.input.clone(),
            request.supports_responses,
            request.model.clone(),
            request.instructions.clone(),
            Some(request.tools.clone()),
            request.temperature,
            request.max_output_tokens,
            StreamCallbacks {
                on_chunk: stream_output
                    .then(|| options.callbacks.on_chunk.clone())
                    .flatten(),
                on_thinking: stream_output
                    .then(|| options.callbacks.on_thinking.clone())
                    .flatten(),
            },
            Some(request.provider.clone()),
            request.thinking_level.clone(),
            on_before_send_model_request,
            AiRequestOptions {
                prompt_cache_key: request.prompt_cache_key.clone(),
                request_cwd: request.request_cwd.clone(),
                include_prompt_cache_retention: request.include_prompt_cache_retention,
                request_body_limit_bytes: request.request_body_limit_bytes,
                abort_token: options.abort_token.clone(),
                force_identity_encoding: false,
            },
        )
        .await;
    let result = result.and_then(|response| {
        if let Some(error) = failed_ai_response_error(&response) {
            Err(error)
        } else {
            Ok(response)
        }
    });
    let model_request_ms = started_at.elapsed().as_millis();
    match &result {
        Ok(response) => {
            info!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                reason = iteration_reason,
                model = request.model.as_str(),
                provider = request.provider.as_str(),
                model_request_ms,
                response_id = response.response_id.as_deref().unwrap_or(""),
                tool_call_count = response
                    .tool_calls
                    .as_ref()
                    .and_then(|value| value.as_array())
                    .map(Vec::len)
                    .unwrap_or_default(),
                "ai runtime model request completed"
            );
        }
        Err(err) => {
            warn!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                reason = iteration_reason,
                model = request.model.as_str(),
                provider = request.provider.as_str(),
                model_request_ms,
                error = err.as_str(),
                "ai runtime model request failed"
            );
        }
    }
    result
}

fn failed_ai_response_error(response: &AiResponse) -> Option<String> {
    let finish_reason = response
        .finish_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let response_failed = finish_reason.is_some_and(|value| value.eq_ignore_ascii_case("failed"));
    if !response_failed && response.provider_error.is_none() {
        return None;
    }

    let mut parts = vec![format!(
        "finish_reason={}",
        finish_reason.unwrap_or("unknown")
    )];
    if let Some(response_id) = response
        .response_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("response_id={response_id}"));
    }
    parts.push(format!(
        "provider_error={}",
        response
            .provider_error
            .as_ref()
            .map(provider_error_detail)
            .unwrap_or_else(|| "unavailable".to_string())
    ));
    Some(format!("ai response failed: {}", parts.join("; ")))
}

fn provider_error_detail(value: &Value) -> String {
    let detail = value
        .as_object()
        .map(|object| {
            ["code", "type", "message"]
                .into_iter()
                .filter_map(|key| {
                    object
                        .get(key)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| format!("{key}={value}"))
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|detail| !detail.is_empty())
        .or_else(|| value.as_str().map(str::trim).map(ToOwned::to_owned))
        .filter(|detail| !detail.is_empty())
        .unwrap_or_else(|| value.to_string());
    truncate_chars(detail.as_str(), PROVIDER_ERROR_DETAIL_MAX_CHARS)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...<truncated>");
    }
    output
}

fn build_before_send_model_request_callback(
    callbacks: &RuntimeCallbacks,
    request_debug: Value,
) -> Option<Arc<dyn Fn(Value) + Send + Sync>> {
    let legacy_callback = callbacks.on_before_model_request.clone();
    let payload_callback = callbacks.on_before_send_model_request.clone();
    if legacy_callback.is_none() && payload_callback.is_none() {
        return None;
    }

    Some(Arc::new(move |payload: Value| {
        if let Some(callback) = &legacy_callback {
            callback(attach_runtime_debug(payload.clone(), &request_debug));
        }
        if let Some(callback) = &payload_callback {
            callback(payload);
        }
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::*;

    #[test]
    fn failed_response_preserves_provider_error_details() {
        let response = AiResponse {
            content: String::new(),
            reasoning: None,
            tool_calls: None,
            finish_reason: Some("failed".to_string()),
            provider_error: Some(json!({
                "code": "server_is_overloaded",
                "type": "server_error",
                "message": "Our servers are currently overloaded."
            })),
            usage: None,
            response_id: Some("resp_1".to_string()),
        };

        let error = failed_ai_response_error(&response).expect("failed response error");

        assert!(error.contains("finish_reason=failed"));
        assert!(error.contains("response_id=resp_1"));
        assert!(error.contains("code=server_is_overloaded"));
        assert!(error.contains("message=Our servers are currently overloaded."));
    }

    #[test]
    fn failed_response_without_provider_error_remains_actionable() {
        let response = AiResponse {
            content: String::new(),
            reasoning: None,
            tool_calls: None,
            finish_reason: Some("failed".to_string()),
            provider_error: None,
            usage: None,
            response_id: None,
        };

        assert_eq!(
            failed_ai_response_error(&response).as_deref(),
            Some("ai response failed: finish_reason=failed; provider_error=unavailable")
        );
    }

    #[test]
    fn before_send_callback_preserves_exact_payload_and_legacy_debug_payload() {
        let legacy_payload = Arc::new(Mutex::new(None));
        let exact_payload = Arc::new(Mutex::new(None));
        let callbacks = RuntimeCallbacks {
            on_before_model_request: Some(Arc::new({
                let legacy_payload = Arc::clone(&legacy_payload);
                move |payload| {
                    *legacy_payload.lock().expect("legacy payload") = Some(payload);
                }
            })),
            on_before_send_model_request: Some(Arc::new({
                let exact_payload = Arc::clone(&exact_payload);
                move |payload| {
                    *exact_payload.lock().expect("exact payload") = Some(payload);
                }
            })),
            ..RuntimeCallbacks::default()
        };
        let callback = build_before_send_model_request_callback(
            &callbacks,
            json!({"iteration": 2, "reason": "tool_results"}),
        )
        .expect("callback");
        let payload = json!({"model": "test", "input": []});

        callback(payload.clone());

        assert_eq!(
            *exact_payload.lock().expect("exact payload"),
            Some(payload.clone())
        );
        let legacy = legacy_payload
            .lock()
            .expect("legacy payload")
            .clone()
            .expect("legacy value");
        assert_eq!(legacy["model"], payload["model"]);
        assert_eq!(legacy["task_runner_debug"]["iteration"], 2);
    }
}
