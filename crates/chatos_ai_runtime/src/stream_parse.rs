// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::response_parse::{
    append_stream_text, extract_output_text, extract_reasoning_from_response, join_stream_text,
    looks_like_response_id,
};

#[path = "stream_parse/text.rs"]
mod text;
#[path = "stream_parse/tool_calls.rs"]
mod tool_calls;

use self::text::{
    extract_chat_delta_text, extract_chat_reasoning_text, extract_reasoning_event_text,
    extract_text_delta, extract_text_from_fields, non_empty_trimmed,
};
pub use self::tool_calls::{collect_stream_tool_calls, extract_responses_tool_calls};
use self::tool_calls::{
    ingest_tool_call_item, ingest_tool_calls_from_response_output, merge_chat_tool_call_delta,
    merge_function_call_arguments_delta, merge_function_call_done,
};

#[derive(Debug, Default, Clone)]
pub struct StreamState {
    pub full_content: String,
    pub reasoning: String,
    pub tool_calls_map: BTreeMap<usize, Value>,
    pub tool_call_index_map: BTreeMap<String, usize>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    pub provider_error: Option<Value>,
    pub response_obj: Option<Value>,
    pub sent_any_chunk: bool,
}

#[derive(Debug, Default, Clone)]
pub struct StreamPayload {
    pub chunk: Option<String>,
    pub thinking: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct FinalizedStreamState {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub provider_error: Option<Value>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
}

pub fn apply_responses_stream_event(state: &mut StreamState, event: &Value) -> StreamPayload {
    let mut payload = StreamPayload::default();

    if let Some(event_type) = event.get("type").and_then(Value::as_str) {
        if event_type == "response.output_text.delta" {
            if let Some(delta) = event.get("delta").and_then(extract_text_delta) {
                if !delta.is_empty() {
                    state.full_content =
                        join_stream_text(state.full_content.as_str(), delta.as_str());
                    state.sent_any_chunk = true;
                    payload.chunk = Some(delta);
                }
            }
        } else if event_type == "response.output_text.done"
            || event_type == "response.output_text"
            || event_type == "response.output_text.completed"
        {
            if state.full_content.is_empty() {
                if let Some(text) =
                    extract_text_from_fields(event, &["text", "output_text", "delta"])
                {
                    if !text.is_empty() {
                        state.full_content.push_str(&text);
                        state.sent_any_chunk = true;
                        payload.chunk = Some(text);
                    }
                }
            }
        } else if event_type == "response.output_item.added"
            || event_type == "response.output_item.delta"
            || event_type == "response.output_item.done"
        {
            let item = event.get("item").or_else(|| event.get("output_item"));
            if let Some(item) = item {
                let extra_arguments_piece = if event_type == "response.output_item.delta" {
                    event
                        .get("delta")
                        .and_then(extract_text_delta)
                        .or_else(|| extract_text_from_fields(event, &["arguments", "text"]))
                } else {
                    None
                };
                ingest_tool_call_item(state, event, item, extra_arguments_piece.as_deref());
            }
        } else if event_type == "response.function_call_arguments.delta"
            || event_type == "response.function_call.delta"
        {
            if let Some(arguments_piece) = event
                .get("delta")
                .and_then(extract_text_delta)
                .or_else(|| extract_text_from_fields(event, &["arguments", "text"]))
            {
                merge_function_call_arguments_delta(state, event, arguments_piece.as_str());
            }
        } else if event_type == "response.function_call_arguments.done"
            || event_type == "response.function_call.done"
        {
            let args_piece = extract_text_from_fields(event, &["arguments", "output", "text"]);
            let name_piece = event.get("name").and_then(Value::as_str);
            merge_function_call_done(state, event, name_piece, args_piece.as_deref());
        } else if let Some(reasoning_delta) = extract_reasoning_event_text(event_type, event) {
            if !reasoning_delta.is_empty() {
                state.reasoning =
                    join_stream_text(state.reasoning.as_str(), reasoning_delta.as_str());
                payload.thinking = Some(reasoning_delta);
            }
        } else if event_type == "response.completed" {
            if let Some(response) = event.get("response") {
                state.response_obj = Some(response.clone());
                ingest_tool_calls_from_response_output(state, response);
                if state.full_content.is_empty() {
                    let extracted = extract_output_text(response);
                    if !extracted.is_empty() {
                        state.full_content =
                            join_stream_text(state.full_content.as_str(), extracted.as_str());
                        state.sent_any_chunk = true;
                        payload.chunk = Some(extracted);
                    }
                }
            } else {
                state.response_obj = Some(event.clone());
                ingest_tool_calls_from_response_output(state, event);
                if state.full_content.is_empty() {
                    let extracted = extract_output_text(event);
                    if !extracted.is_empty() {
                        state.full_content =
                            join_stream_text(state.full_content.as_str(), extracted.as_str());
                        state.sent_any_chunk = true;
                        payload.chunk = Some(extracted);
                    }
                }
            }
        } else if event_type == "response.failed" {
            state.finish_reason = Some("failed".to_string());
            if let Some(response) = event.get("response") {
                state.response_obj = Some(response.clone());
                if let Some(error_obj) = response.get("error") {
                    if !error_obj.is_null() {
                        state.provider_error = Some(error_obj.clone());
                    }
                }
            }
            if let Some(error_obj) = event.get("error") {
                if !error_obj.is_null() {
                    state.provider_error = Some(error_obj.clone());
                }
            }
        } else if event_type == "error" {
            if let Some(error_obj) = event.get("error") {
                if !error_obj.is_null() {
                    state.provider_error = Some(error_obj.clone());
                }
            }
        } else if state.response_obj.is_none() {
            if let Some(response) = event.get("response") {
                if response.get("output").is_some()
                    || response.get("output_text").is_some()
                    || response.get("status").is_some()
                {
                    state.response_obj = Some(response.clone());
                }
            } else if event.get("output").is_some() || event.get("output_text").is_some() {
                state.response_obj = Some(event.clone());
            }
        }
    } else {
        if state.response_obj.is_none()
            && (event.get("output").is_some()
                || event.get("output_text").is_some()
                || event.get("text").is_some()
                || event.get("status").is_some()
                || event.get("error").is_some())
        {
            state.response_obj = Some(event.clone());
        }

        if state.full_content.is_empty() {
            let extracted = extract_output_text(event);
            if !extracted.is_empty() {
                state.full_content =
                    join_stream_text(state.full_content.as_str(), extracted.as_str());
                state.sent_any_chunk = true;
                payload.chunk = Some(extracted);
            }
        }

        if state.finish_reason.is_none() {
            if let Some(status) = event.get("status").and_then(Value::as_str) {
                let normalized = status.trim();
                if !normalized.is_empty() {
                    state.finish_reason = Some(normalized.to_string());
                }
            }
        }
    }

    if let Some(id) = event
        .get("response")
        .and_then(|response| response.get("id"))
        .and_then(Value::as_str)
    {
        state.response_id = Some(id.to_string());
    } else if state.response_id.is_none() {
        if let Some(id) = event.get("id").and_then(Value::as_str) {
            if looks_like_response_id(id) {
                state.response_id = Some(id.to_string());
            }
        }
    }

    if let Some(usage) = event
        .get("response")
        .and_then(|response| response.get("usage"))
    {
        state.usage = Some(usage.clone());
    }

    if state.provider_error.is_none() {
        if let Some(error_obj) = event
            .get("response")
            .and_then(|response| response.get("error"))
        {
            if !error_obj.is_null() {
                state.provider_error = Some(error_obj.clone());
            }
        } else if let Some(error_obj) = event.get("error") {
            if !error_obj.is_null() {
                state.provider_error = Some(error_obj.clone());
            }
        }
    }

    payload
}

pub fn apply_chat_completions_stream_event(
    state: &mut StreamState,
    event: &Value,
    reasoning_enabled: bool,
) -> StreamPayload {
    if state.response_id.is_none() {
        state.response_id = event
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    if let Some(usage) = event.get("usage") {
        state.usage = Some(usage.clone());
    }

    let mut payload = StreamPayload::default();
    let Some(choice) = event
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return payload;
    };
    if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
        state.finish_reason = Some(finish_reason.to_string());
    }

    let delta = choice.get("delta").or_else(|| choice.get("message"));
    if let Some(delta) = delta {
        if let Some(content) = extract_chat_delta_text(delta) {
            append_stream_text(&mut state.full_content, content.as_str());
            state.sent_any_chunk = true;
            payload.chunk = Some(content);
        }
        if reasoning_enabled {
            if let Some(reasoning) = extract_chat_reasoning_text(delta) {
                append_stream_text(&mut state.reasoning, reasoning.as_str());
                payload.thinking = Some(reasoning);
            }
        }
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for (fallback_index, tool_call) in tool_calls.iter().enumerate() {
                merge_chat_tool_call_delta(state, fallback_index, tool_call);
            }
        }
    }

    payload
}

pub fn finalize_responses_stream_state(state: &mut StreamState) -> FinalizedStreamState {
    if state.full_content.is_empty() {
        if let Some(response_obj) = &state.response_obj {
            state.full_content = extract_output_text(response_obj);
        }
    }
    if state.reasoning.is_empty() {
        if let Some(response_obj) = &state.response_obj {
            state.reasoning = extract_reasoning_from_response(response_obj);
        }
    }

    let response_val = state
        .response_obj
        .clone()
        .unwrap_or_else(|| json!({ "output_text": state.full_content }));
    if state.finish_reason.is_none() {
        state.finish_reason = response_val
            .get("status")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    if state.response_id.is_none() {
        state.response_id = response_val
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    if state.usage.is_none() {
        state.usage = response_val.get("usage").cloned();
    }
    if state.provider_error.is_none() {
        state.provider_error = response_val
            .get("error")
            .cloned()
            .filter(|value| !value.is_null());
    }

    FinalizedStreamState {
        content: state.full_content.clone(),
        reasoning: non_empty_trimmed(state.reasoning.as_str()),
        tool_calls: extract_responses_tool_calls(&response_val)
            .or_else(|| collect_stream_tool_calls(&state.tool_calls_map)),
        finish_reason: state.finish_reason.clone(),
        provider_error: state.provider_error.clone(),
        usage: state.usage.clone(),
        response_id: state.response_id.clone(),
    }
}

pub fn finalize_chat_completions_stream_state(state: &mut StreamState) -> FinalizedStreamState {
    FinalizedStreamState {
        content: state.full_content.clone(),
        reasoning: non_empty_trimmed(state.reasoning.as_str()),
        tool_calls: collect_stream_tool_calls(&state.tool_calls_map),
        finish_reason: state.finish_reason.clone(),
        provider_error: state.provider_error.clone(),
        usage: state.usage.clone(),
        response_id: state.response_id.clone(),
    }
}
