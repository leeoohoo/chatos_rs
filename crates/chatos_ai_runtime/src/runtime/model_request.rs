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
                abort_token: None,
                force_identity_encoding: false,
            },
        )
        .await;
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
