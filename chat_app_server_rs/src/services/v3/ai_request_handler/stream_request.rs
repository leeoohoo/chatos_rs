use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::core::messages::owned_non_empty_text;
use crate::services::ai_common::{
    consume_sse_stream, emit_stream_callbacks, parsed_stream_response_is_empty,
    read_error_response_text, send_bearer_json_request, EMPTY_STREAM_RESPONSE_PARSE_ERROR,
};

use super::parser::{
    apply_stream_event, collect_stream_tool_calls, extract_output_text,
    extract_reasoning_from_response, extract_tool_calls, StreamState,
};
use super::{persist_assistant_response_if_needed, AiRequestHandler, AiResponse, StreamCallbacks};

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
            error!("[AI_V3] stream request failed: {}", err);
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
            "[AI_V3][prev-id] stream response parsed: session_id={}, turn_id={}, response_id={}, tool_call_count={}",
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
