// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{AiResponse, AiTransport, StreamCallbacks};
use crate::model_config::{reasoning_effort_for_provider, thinking_mode_for_provider};
use crate::stream::consume_sse_stream;
use crate::stream_parse::{
    apply_chat_completions_stream_event, apply_responses_stream_event,
    finalize_chat_completions_stream_state, finalize_responses_stream_state, FinalizedStreamState,
    StreamState,
};
use crate::tool_call::collect_ordered_tool_calls;

const EMPTY_STREAM_RESPONSE_PARSE_ERROR: &str =
    "stream response parse failed: no valid SSE events parsed from provider";

pub(super) async fn parse_stream_response(
    response: reqwest::Response,
    transport: AiTransport,
    callbacks: StreamCallbacks,
    provider: Option<&str>,
    thinking_level: Option<&str>,
    abort_token: Option<CancellationToken>,
) -> Result<AiResponse, String> {
    let mut state = StreamState::default();
    let mut parsed_event_count = 0usize;
    let mut sent_any_thinking = false;
    let reasoning_enabled = reasoning_effort_for_provider(provider, thinking_level).is_some()
        || thinking_mode_for_provider(provider, thinking_level) == Some("enabled");

    consume_sse_stream(response.bytes_stream(), abort_token, |event| {
        parsed_event_count += 1;
        let payload = match transport {
            AiTransport::Responses => apply_responses_stream_event(&mut state, &event),
            AiTransport::ChatCompletions => {
                apply_chat_completions_stream_event(&mut state, &event, reasoning_enabled)
            }
        };
        if let Some(chunk) = payload.chunk {
            if let Some(cb) = &callbacks.on_chunk {
                cb(chunk);
            }
        }
        if let Some(thinking) = payload.thinking {
            sent_any_thinking = true;
            if let Some(cb) = &callbacks.on_thinking {
                cb(thinking);
            }
        }
    })
    .await?;

    if parsed_stream_response_is_empty(parsed_event_count, &state) {
        return Err(EMPTY_STREAM_RESPONSE_PARSE_ERROR.to_string());
    }

    let finalized = match transport {
        AiTransport::Responses => finalize_responses_stream_state(&mut state),
        AiTransport::ChatCompletions => finalize_chat_completions_stream_state(&mut state),
    };

    emit_finalized_stream_callbacks(
        &finalized,
        state.sent_any_chunk,
        sent_any_thinking,
        &callbacks,
    );

    Ok(AiResponse {
        content: finalized.content,
        reasoning: finalized.reasoning,
        tool_calls: match transport {
            AiTransport::Responses => finalized.tool_calls,
            AiTransport::ChatCompletions => {
                collect_tool_calls(&state.tool_calls_map).or(finalized.tool_calls)
            }
        },
        finish_reason: finalized.finish_reason,
        provider_error: finalized.provider_error,
        usage: finalized.usage,
        response_id: finalized.response_id,
    })
}

pub(super) fn emit_finalized_stream_callbacks(
    finalized: &FinalizedStreamState,
    sent_any_chunk: bool,
    sent_any_thinking: bool,
    callbacks: &StreamCallbacks,
) {
    if !sent_any_chunk && !finalized.content.is_empty() {
        if let Some(cb) = &callbacks.on_chunk {
            cb(finalized.content.clone());
        }
    }

    if sent_any_thinking {
        return;
    }
    if let Some(reasoning) = finalized.reasoning.as_deref().map(str::trim) {
        if !reasoning.is_empty() {
            if let Some(cb) = &callbacks.on_thinking {
                cb(reasoning.to_string());
            }
        }
    }
}

fn collect_tool_calls(tool_calls: &BTreeMap<usize, Value>) -> Option<Value> {
    collect_ordered_tool_calls(tool_calls).and_then(|value| {
        let calls = value
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|item| {
                item.get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some()
            })
            .collect::<Vec<_>>();
        if calls.is_empty() {
            None
        } else {
            Some(Value::Array(calls))
        }
    })
}

fn parsed_stream_response_is_empty(parsed_event_count: usize, state: &StreamState) -> bool {
    parsed_event_count == 0
        && state.full_content.trim().is_empty()
        && state.reasoning.trim().is_empty()
        && state.tool_calls_map.is_empty()
        && state.response_obj.is_none()
        && state.provider_error.is_none()
}
