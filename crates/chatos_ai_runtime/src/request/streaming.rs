// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::warn;

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
    let response_stream = response
        .bytes_stream()
        .map(|chunk| chunk.map_err(super::http::format_reqwest_error));
    parse_stream_chunks(
        response_stream,
        transport,
        callbacks,
        provider,
        thinking_level,
        abort_token,
    )
    .await
}

async fn parse_stream_chunks<S, E>(
    response_stream: S,
    transport: AiTransport,
    callbacks: StreamCallbacks,
    provider: Option<&str>,
    thinking_level: Option<&str>,
    abort_token: Option<CancellationToken>,
) -> Result<AiResponse, String>
where
    S: Stream<Item = Result<bytes::Bytes, E>> + Unpin,
    E: ToString,
{
    let mut state = StreamState::default();
    let mut sent_any_thinking = false;
    let reasoning_enabled = reasoning_effort_for_provider(provider, thinking_level).is_some()
        || thinking_mode_for_provider(provider, thinking_level) == Some("enabled");

    let stream_result = consume_sse_stream(response_stream, abort_token, |event| {
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
    .await;

    let stream_stats = match stream_result {
        Ok(stats) => stats,
        Err(error) => {
            if error.message == "aborted" {
                return Err(error.message);
            }
            if !stream_state_is_safely_completed(
                &state,
                transport,
                error.stats.malformed_event_count,
            ) {
                return Err(format!(
                    "stream response body failed after {} valid events (malformed_events={}, buffered_tail_bytes={}): {}",
                    error.stats.parsed_event_count,
                    error.stats.malformed_event_count,
                    error.stats.buffered_tail_bytes,
                    error.message
                ));
            }
            warn!(
                transport = ?transport,
                parsed_event_count = error.stats.parsed_event_count,
                malformed_event_count = error.stats.malformed_event_count,
                buffered_tail_bytes = error.stats.buffered_tail_bytes,
                error = error.message.as_str(),
                "recovered completed AI response after trailing stream body failure"
            );
            error.stats
        }
    };

    if stream_stats.malformed_event_count > 0
        && !stream_state_is_safely_completed(&state, transport, stream_stats.malformed_event_count)
    {
        return Err(format!(
            "stream response parse failed: {} malformed SSE event(s) after {} valid event(s)",
            stream_stats.malformed_event_count, stream_stats.parsed_event_count
        ));
    }

    if parsed_stream_response_is_empty(stream_stats.parsed_event_count, &state) {
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
        response_output_items: finalized.response_output_items,
    })
}

fn stream_state_is_safely_completed(
    state: &StreamState,
    transport: AiTransport,
    malformed_event_count: usize,
) -> bool {
    if state.provider_error.is_some() {
        return false;
    }
    match transport {
        AiTransport::Responses => state.response_obj.as_ref().is_some_and(|response| {
            response
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case("completed"))
                && response_function_calls_are_complete(response)
        }),
        AiTransport::ChatCompletions => {
            malformed_event_count == 0
                && state.finish_reason.as_deref().is_some_and(|reason| {
                    let reason = reason.trim();
                    !reason.is_empty() && !reason.eq_ignore_ascii_case("failed")
                })
                && collected_tool_calls_are_complete(&state.tool_calls_map)
        }
    }
}

fn response_function_calls_are_complete(response: &Value) -> bool {
    response
        .get("output")
        .and_then(Value::as_array)
        .is_none_or(|items| {
            items.iter().all(|item| {
                item.get("type").and_then(Value::as_str) != Some("function_call")
                    || complete_function_call(
                        item.get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str),
                        item.get("name").and_then(Value::as_str),
                        item.get("arguments"),
                    )
            })
        })
}

fn collected_tool_calls_are_complete(tool_calls: &BTreeMap<usize, Value>) -> bool {
    tool_calls.values().all(|tool_call| {
        let function = tool_call.get("function");
        complete_function_call(
            tool_call.get("id").and_then(Value::as_str),
            function
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str),
            function.and_then(|value| value.get("arguments")),
        )
    })
}

fn complete_function_call(
    call_id: Option<&str>,
    name: Option<&str>,
    arguments: Option<&Value>,
) -> bool {
    let has_call_id = call_id
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let has_name = name.map(str::trim).is_some_and(|value| !value.is_empty());
    let valid_arguments = arguments.is_some_and(|value| match value {
        Value::Object(_) => true,
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .is_some_and(|parsed| parsed.is_object()),
        _ => false,
    });
    has_call_id && has_name && valid_arguments
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

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures::stream;
    use serde_json::json;

    use super::{parse_stream_chunks, AiTransport, StreamCallbacks};

    #[tokio::test]
    async fn completed_responses_event_survives_trailing_body_error() {
        let completed = json!({
            "type": "response.completed",
            "response": {
                "id": "resp_complete",
                "status": "completed",
                "output_text": "finished safely",
                "output": []
            }
        });
        let chunks = vec![
            Ok::<Bytes, String>(Bytes::from(format!("data: {completed}\n\n"))),
            Err("AI transport error (kind=body): operation timed out".to_string()),
        ];

        let response = parse_stream_chunks(
            stream::iter(chunks),
            AiTransport::Responses,
            StreamCallbacks::default(),
            Some("openai"),
            None,
            None,
        )
        .await
        .expect("completed response should be recovered");

        assert_eq!(response.content, "finished safely");
        assert_eq!(response.finish_reason.as_deref(), Some("completed"));
        assert_eq!(response.response_id.as_deref(), Some("resp_complete"));
    }

    #[tokio::test]
    async fn incomplete_tool_arguments_are_not_recovered_after_body_error() {
        let completed = json!({
            "type": "response.completed",
            "response": {
                "id": "resp_incomplete",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "create_task",
                    "arguments": "{\"title\":"
                }]
            }
        });
        let chunks = vec![
            Ok::<Bytes, String>(Bytes::from(format!("data: {completed}\n\n"))),
            Err("AI transport error (kind=body): unexpected eof".to_string()),
        ];

        let error = parse_stream_chunks(
            stream::iter(chunks),
            AiTransport::Responses,
            StreamCallbacks::default(),
            Some("openai"),
            None,
            None,
        )
        .await
        .expect_err("incomplete tool arguments must remain a failure");

        assert!(error.contains("stream response body failed"));
    }

    #[tokio::test]
    async fn malformed_sse_packets_are_reported_instead_of_silently_dropped() {
        let chunks = vec![Ok::<Bytes, String>(Bytes::from(concat!(
            "data: {bad json}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n"
        )))];

        let error = parse_stream_chunks(
            stream::iter(chunks),
            AiTransport::Responses,
            StreamCallbacks::default(),
            Some("openai"),
            None,
            None,
        )
        .await
        .expect_err("malformed packet should be diagnostic");

        assert!(error.contains("1 malformed SSE event"));
        assert!(error.contains("1 valid event"));
    }

    #[tokio::test]
    async fn trailing_plain_json_non_stream_response_still_parses() {
        let chunks = vec![Ok::<Bytes, String>(Bytes::from(
            json!({
                "id": "resp_json",
                "status": "completed",
                "output_text": "plain response",
                "output": []
            })
            .to_string(),
        ))];

        let response = parse_stream_chunks(
            stream::iter(chunks),
            AiTransport::Responses,
            StreamCallbacks::default(),
            Some("openai"),
            None,
            None,
        )
        .await
        .expect("plain JSON response should parse");

        assert_eq!(response.content, "plain response");
        assert_eq!(response.finish_reason.as_deref(), Some("completed"));
    }
}
