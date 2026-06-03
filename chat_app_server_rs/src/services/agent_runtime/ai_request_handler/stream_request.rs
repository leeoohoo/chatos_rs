use std::collections::BTreeMap;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::core::messages::{owned_non_empty_text, text_value_or_json};
use crate::core::tool_call::{collect_ordered_tool_calls, merge_indexed_tool_call_parts};
use crate::services::ai_common::{
    consume_sse_stream, emit_stream_callbacks, parsed_stream_response_is_empty,
    read_error_response_text, send_bearer_json_request, EMPTY_STREAM_RESPONSE_PARSE_ERROR,
};

use super::parser::{
    apply_stream_event, collect_stream_tool_calls, extract_output_text,
    extract_reasoning_from_response, extract_tool_calls, StreamState,
};
use super::{persist_assistant_response_if_needed, AiRequestHandler, AiResponse, StreamCallbacks};

#[derive(Debug, Default)]
struct ChatCompletionsStreamState {
    full_content: String,
    reasoning: String,
    tool_calls_map: BTreeMap<usize, Value>,
    finish_reason: Option<String>,
    usage: Option<Value>,
    response_id: Option<String>,
    sent_any_chunk: bool,
}

#[derive(Debug, Default)]
struct ChatCompletionsStreamPayload {
    chunk: Option<String>,
    thinking: Option<String>,
}

impl AiRequestHandler {
    pub(super) async fn handle_stream_request(
        &self,
        url: String,
        payload: serde_json::Value,
        callbacks: StreamCallbacks,
        session_id: Option<String>,
        turn_id: Option<String>,
        token: Option<CancellationToken>,
        force_identity_encoding: bool,
        persist_messages: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<AiResponse, String> {
        let resp = send_bearer_json_request(
            &self.client,
            &url,
            &self.api_key,
            &payload,
            token.clone(),
            force_identity_encoding,
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let err = read_error_response_text(resp)
                .await
                .unwrap_or_else(|inner| format!("status {}: {}", status, inner));
            error!("[Agent Runtime] stream request failed: {}", err);
            return Err(err);
        }

        let stream = resp.bytes_stream();
        let mut stream_state = StreamState::default();
        let mut parsed_event_count: usize = 0;

        consume_sse_stream(stream, token.clone(), |v| {
            parsed_event_count += 1;
            let payload = apply_stream_event(&mut stream_state, &v);
            emit_stream_callbacks(&callbacks, payload.chunk, payload.thinking);
        })
        .await?;

        let parsed_empty_response = parsed_stream_response_is_empty(
            parsed_event_count,
            stream_state.full_content.as_str(),
            stream_state.reasoning.as_str(),
            stream_state.response_obj.is_some() || stream_state.provider_error.is_some(),
        );
        if parsed_empty_response {
            return Err(EMPTY_STREAM_RESPONSE_PARSE_ERROR.to_string());
        }

        let response_val = stream_state
            .response_obj
            .clone()
            .unwrap_or_else(|| json!({ "output_text": stream_state.full_content }));
        let tool_calls = extract_tool_calls(&response_val)
            .or_else(|| collect_stream_tool_calls(&stream_state.tool_calls_map));
        let content = if !stream_state.full_content.is_empty() {
            stream_state.full_content.clone()
        } else {
            extract_output_text(&response_val)
        };
        if !stream_state.sent_any_chunk {
            if let Some(cb) = &callbacks.on_chunk {
                if !content.is_empty() {
                    cb(content.clone());
                }
            }
        }
        let reasoning_opt = owned_non_empty_text(stream_state.reasoning.as_str()).or_else(|| {
            let fallback = extract_reasoning_from_response(&response_val);
            owned_non_empty_text(fallback.as_str())
        });
        if stream_state.finish_reason.is_none() {
            stream_state.finish_reason = response_val
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if stream_state.response_id.is_none() {
            stream_state.response_id = response_val
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        info!(
            "[Agent Runtime][prev-id] stream response parsed: session_id={}, turn_id={}, response_id={}, tool_call_count={}",
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            turn_id.clone().unwrap_or_else(|| "n/a".to_string()),
            stream_state.response_id.as_deref().unwrap_or("none"),
            tool_calls
                .as_ref()
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0)
        );
        if stream_state.usage.is_none() {
            stream_state.usage = response_val.get("usage").cloned();
        }
        if stream_state.provider_error.is_none() {
            stream_state.provider_error =
                response_val.get("error").cloned().filter(|v| !v.is_null());
        }

        persist_assistant_response_if_needed(
            self,
            session_id.clone(),
            turn_id.clone(),
            persist_messages,
            message_mode,
            message_source,
            metadata,
            content.as_str(),
            reasoning_opt.clone(),
            tool_calls.clone(),
            stream_state.response_id.clone(),
            stream_state.finish_reason.clone(),
            "non-terminal empty stream response",
        )
        .await;

        Ok(AiResponse {
            content,
            reasoning: reasoning_opt,
            tool_calls,
            finish_reason: stream_state.finish_reason,
            provider_error: stream_state.provider_error,
            usage: stream_state.usage,
            response_id: stream_state.response_id,
        })
    }
}

impl AiRequestHandler {
    pub(super) async fn handle_chat_completions_stream_request(
        &self,
        url: String,
        payload: Value,
        callbacks: StreamCallbacks,
        provider: Option<String>,
        thinking_level: Option<String>,
        session_id: Option<String>,
        turn_id: Option<String>,
        token: Option<CancellationToken>,
        force_identity_encoding: bool,
        persist_messages: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
    ) -> Result<AiResponse, String> {
        let resp = send_bearer_json_request(
            &self.client,
            &url,
            &self.api_key,
            &payload,
            token.clone(),
            force_identity_encoding,
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let err = read_error_response_text(resp)
                .await
                .unwrap_or_else(|inner| format!("status {}: {}", status, inner));
            error!(
                "[Agent Runtime] chat-completions stream request failed: {}",
                err
            );
            return Err(err);
        }

        let stream = resp.bytes_stream();
        let mut stream_state = ChatCompletionsStreamState::default();
        let mut parsed_event_count: usize = 0;
        let reasoning_enabled = crate::services::ai_common::normalize_reasoning_effort(
            provider.as_deref(),
            thinking_level.as_deref(),
        )
        .is_some();

        consume_sse_stream(stream, token.clone(), |event| {
            parsed_event_count += 1;
            let payload =
                apply_chat_completions_stream_event(&mut stream_state, &event, reasoning_enabled);
            emit_stream_callbacks(&callbacks, payload.chunk, payload.thinking);
        })
        .await?;

        let has_tool_calls = !stream_state.tool_calls_map.is_empty();
        let parsed_empty_response = parsed_stream_response_is_empty(
            parsed_event_count,
            stream_state.full_content.as_str(),
            stream_state.reasoning.as_str(),
            has_tool_calls,
        );
        if parsed_empty_response {
            return Err(EMPTY_STREAM_RESPONSE_PARSE_ERROR.to_string());
        }

        let tool_calls = collect_ordered_tool_calls(&stream_state.tool_calls_map);
        let reasoning_opt = owned_non_empty_text(stream_state.reasoning.as_str());
        if !stream_state.sent_any_chunk {
            if let Some(cb) = &callbacks.on_chunk {
                if !stream_state.full_content.is_empty() {
                    cb(stream_state.full_content.clone());
                }
            }
        }

        info!(
            "[Agent Runtime][chat-completions] stream response parsed: session_id={}, turn_id={}, response_id={}, tool_call_count={}",
            session_id.clone().unwrap_or_else(|| "n/a".to_string()),
            turn_id.clone().unwrap_or_else(|| "n/a".to_string()),
            stream_state.response_id.as_deref().unwrap_or("none"),
            tool_calls
                .as_ref()
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0)
        );

        persist_assistant_response_if_needed(
            self,
            session_id.clone(),
            turn_id.clone(),
            persist_messages,
            message_mode,
            message_source,
            metadata,
            stream_state.full_content.as_str(),
            reasoning_opt.clone(),
            tool_calls.clone(),
            stream_state.response_id.clone(),
            stream_state.finish_reason.clone(),
            "chat-completions empty stream response",
        )
        .await;

        Ok(AiResponse {
            content: stream_state.full_content,
            reasoning: reasoning_opt,
            tool_calls,
            finish_reason: stream_state.finish_reason,
            provider_error: None,
            usage: stream_state.usage,
            response_id: stream_state.response_id,
        })
    }
}

fn apply_chat_completions_stream_event(
    state: &mut ChatCompletionsStreamState,
    event: &Value,
    reasoning_enabled: bool,
) -> ChatCompletionsStreamPayload {
    let mut payload = ChatCompletionsStreamPayload::default();

    if state.response_id.is_none() {
        state.response_id = event
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    if let Some(usage) = event.get("usage") {
        state.usage = Some(usage.clone());
    }

    let Some(choice) = event.get("choices").and_then(|choices| choices.get(0)) else {
        return payload;
    };
    if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
        state.finish_reason = Some(finish_reason.to_string());
    }

    if let Some(delta) = choice.get("delta") {
        apply_chat_message_delta(state, delta, reasoning_enabled, &mut payload);
        return payload;
    }
    if let Some(message) = choice.get("message") {
        apply_chat_message_delta(state, message, reasoning_enabled, &mut payload);
    }

    payload
}

fn apply_chat_message_delta(
    state: &mut ChatCompletionsStreamState,
    delta: &Value,
    reasoning_enabled: bool,
    payload: &mut ChatCompletionsStreamPayload,
) {
    let content = delta
        .get("content")
        .and_then(|value| extract_chat_text(value))
        .unwrap_or_default();
    if !content.is_empty() {
        state.full_content =
            crate::core::tool_call::join_stream_text(state.full_content.as_str(), content.as_str());
        state.sent_any_chunk = true;
        payload.chunk = Some(content);
    }

    if reasoning_enabled {
        let reasoning_piece = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(|value| extract_chat_text(value))
            .unwrap_or_default();
        if !reasoning_piece.is_empty() {
            state.reasoning = crate::core::tool_call::join_stream_text(
                state.reasoning.as_str(),
                reasoning_piece.as_str(),
            );
            payload.thinking = Some(reasoning_piece);
        }
    }

    if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
        for (fallback_index, tool_call) in tool_calls.iter().enumerate() {
            merge_chat_tool_call_delta(state, fallback_index, tool_call);
        }
    }
}

fn merge_chat_tool_call_delta(
    state: &mut ChatCompletionsStreamState,
    fallback_index: usize,
    tool_call: &Value,
) {
    let index = tool_call
        .get("index")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(fallback_index);
    let id = tool_call.get("id").and_then(Value::as_str);
    let call_id = tool_call.get("call_id").and_then(Value::as_str);
    let function = tool_call.get("function");
    let name_piece = function
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .or_else(|| tool_call.get("name").and_then(Value::as_str));
    let arguments_piece = function
        .and_then(|value| value.get("arguments"))
        .and_then(Value::as_str)
        .or_else(|| tool_call.get("arguments").and_then(Value::as_str));

    merge_indexed_tool_call_parts(
        &mut state.tool_calls_map,
        index,
        id,
        call_id,
        name_piece,
        arguments_piece,
    );
}

fn extract_chat_text(value: &Value) -> Option<String> {
    let text = text_value_or_json(value, &["text", "value", "content", "delta"]);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
