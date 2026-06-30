use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::request::{AiRequestHandler, AiRequestOptions, AiResponse, StreamCallbacks};
use crate::traits::ModelRequest;

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
    let on_before_model_request = options
        .callbacks
        .on_before_model_request
        .as_ref()
        .map(|cb| {
            let cb = Arc::clone(cb);
            let request_debug = request_debug.clone();
            Arc::new(move |payload: Value| {
                cb(attach_runtime_debug(payload, &request_debug));
            }) as Arc<dyn Fn(Value) + Send + Sync>
        });

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
                on_chunk: options.callbacks.on_chunk.clone(),
                on_thinking: options.callbacks.on_thinking.clone(),
            },
            Some(request.provider.clone()),
            request.thinking_level.clone(),
            on_before_model_request,
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
