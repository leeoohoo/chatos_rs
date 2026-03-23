use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::services::ai_common::{
    await_with_optional_abort, build_assistant_message_metadata, build_bearer_post_request,
    consume_sse_stream, truncate_log,
};

use super::parser::{
    apply_stream_event, collect_stream_tool_calls, extract_output_text,
    extract_reasoning_from_response, extract_tool_calls, StreamState,
};
use super::{AiRequestHandler, AiResponse, StreamCallbacks};

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
    ) -> Result<AiResponse, String> {
        let req =
            build_bearer_post_request(&self.client, &url, &self.api_key, force_identity_encoding);
        let resp = await_with_optional_abort(req.json(&payload).send(), token.clone()).await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let err_text = truncate_log(&text, 2000);
            error!(
                "[AI_V3] stream request failed: status={}, error={}",
                status, err_text
            );
            return Err(format!("status {}: {}", status, err_text));
        }

        let stream = resp.bytes_stream();
        let mut stream_state = StreamState::default();
        let mut parsed_event_count: usize = 0;

        consume_sse_stream(stream, token.clone(), |v| {
            parsed_event_count += 1;
            let payload = apply_stream_event(&mut stream_state, &v);
            if let Some(chunk) = payload.chunk {
                if let Some(cb) = &callbacks.on_chunk {
                    cb(chunk);
                }
            }
            if let Some(thinking) = payload.thinking {
                if let Some(cb) = &callbacks.on_thinking {
                    cb(thinking);
                }
            }
        })
        .await?;

        let parsed_empty_response = parsed_event_count == 0
            && stream_state.response_obj.is_none()
            && stream_state.full_content.trim().is_empty()
            && stream_state.reasoning.trim().is_empty()
            && stream_state.provider_error.is_none();
        if parsed_empty_response {
            return Err(
                "stream response parse failed: no valid SSE events parsed from provider"
                    .to_string(),
            );
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
        let reasoning_opt = if stream_state.reasoning.is_empty() {
            let fallback = extract_reasoning_from_response(&response_val);
            if fallback.is_empty() {
                None
            } else {
                Some(fallback)
            }
        } else {
            Some(stream_state.reasoning.clone())
        };
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

        if persist_messages {
            if let Some(session_id) = session_id.clone() {
                let meta_val = build_assistant_message_metadata(
                    tool_calls.as_ref(),
                    stream_state.response_id.as_deref(),
                    turn_id.as_deref(),
                );
                if let Err(err) = self
                    .message_manager
                    .save_assistant_message(
                        &session_id,
                        &content,
                        None,
                        reasoning_opt.clone(),
                        message_mode,
                        message_source,
                        meta_val,
                        tool_calls.clone(),
                    )
                    .await
                {
                    error!(
                        "[AI_V3] save assistant message failed: session_id={}, detail={}",
                        session_id, err
                    );
                }
            }
        }

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
