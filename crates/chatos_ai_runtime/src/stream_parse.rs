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
    extract_reasoning_event_text, extract_text_delta, extract_text_from_fields, non_empty_trimmed,
};
pub use self::tool_calls::{collect_stream_tool_calls, extract_responses_tool_calls};
use self::tool_calls::{
    ingest_tool_call_item, ingest_tool_calls_from_response_output,
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
    pub response_status: Option<String>,
    pub incomplete_details: Option<Value>,
    pub terminal_event_type: Option<String>,
    pub terminal_event_seen: bool,
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
    pub response_output_items: Vec<Value>,
    pub response_status: Option<String>,
    pub incomplete_details: Option<Value>,
    pub terminal_event_type: Option<String>,
    pub terminal_event_seen: bool,
}

pub fn apply_responses_stream_event(state: &mut StreamState, event: &Value) -> StreamPayload {
    let mut payload = StreamPayload::default();
    if let Some(event_type) = event.get("type").and_then(Value::as_str) {
        apply_typed_event(state, event, event_type, &mut payload);
    } else {
        apply_untyped_event(state, event, &mut payload);
    }

    capture_event_metadata(state, event);
    payload
}

fn apply_typed_event(
    state: &mut StreamState,
    event: &Value,
    event_type: &str,
    payload: &mut StreamPayload,
) {
    match event_type {
        "response.output_text.delta" => apply_text_delta(state, event, payload),
        "response.output_text.done" | "response.output_text" | "response.output_text.completed" => {
            let text = extract_text_from_fields(event, &["text", "output_text", "delta"]);
            capture_initial_content(state, payload, text);
        }
        "response.output_item.added"
        | "response.output_item.delta"
        | "response.output_item.done" => apply_output_item_event(state, event, event_type),
        "response.function_call_arguments.delta" | "response.function_call.delta" => {
            apply_function_call_delta(state, event)
        }
        "response.function_call_arguments.done" | "response.function_call.done" => {
            let args = extract_text_from_fields(event, &["arguments", "output", "text"]);
            merge_function_call_done(
                state,
                event,
                event.get("name").and_then(Value::as_str),
                args.as_deref(),
            );
        }
        "response.completed" | "response.incomplete" => {
            apply_completed_event(state, event, event_type, payload)
        }
        "response.failed" => apply_failed_event(state, event, event_type),
        "error" => capture_provider_error(state, event.get("error")),
        _ => {
            if let Some(reasoning) = extract_reasoning_event_text(event_type, event) {
                apply_reasoning_event(state, event_type, reasoning, payload);
            } else {
                capture_response_candidate(state, event);
            }
        }
    }
}

fn apply_text_delta(state: &mut StreamState, event: &Value, payload: &mut StreamPayload) {
    let Some(delta) = event.get("delta").and_then(extract_text_delta) else {
        return;
    };
    if !delta.is_empty() {
        append_stream_text(&mut state.full_content, delta.as_str());
        state.sent_any_chunk = true;
        payload.chunk = Some(delta);
    }
}

fn apply_output_item_event(state: &mut StreamState, event: &Value, event_type: &str) {
    let Some(item) = event.get("item").or_else(|| event.get("output_item")) else {
        return;
    };
    let extra_arguments = (event_type == "response.output_item.delta")
        .then(|| {
            event
                .get("delta")
                .and_then(extract_text_delta)
                .or_else(|| extract_text_from_fields(event, &["arguments", "text"]))
        })
        .flatten();
    ingest_tool_call_item(state, event, item, extra_arguments.as_deref());
}

fn apply_function_call_delta(state: &mut StreamState, event: &Value) {
    let arguments = event
        .get("delta")
        .and_then(extract_text_delta)
        .or_else(|| extract_text_from_fields(event, &["arguments", "text"]));
    if let Some(arguments) = arguments {
        merge_function_call_arguments_delta(state, event, arguments.as_str());
    }
}

fn apply_reasoning_event(
    state: &mut StreamState,
    event_type: &str,
    reasoning: String,
    payload: &mut StreamPayload,
) {
    if reasoning.is_empty() {
        return;
    }
    if event_type.ends_with(".delta") {
        append_stream_text(&mut state.reasoning, reasoning.as_str());
    } else {
        state.reasoning = join_stream_text(state.reasoning.as_str(), reasoning.as_str());
    }
    payload.thinking = Some(reasoning);
}

fn apply_completed_event(
    state: &mut StreamState,
    event: &Value,
    event_type: &str,
    payload: &mut StreamPayload,
) {
    state.terminal_event_seen = true;
    state.terminal_event_type = Some(event_type.to_string());
    let response = event.get("response").unwrap_or(event);
    state.response_obj = Some(response.clone());
    capture_response_lifecycle(state, response);
    ingest_tool_calls_from_response_output(state, response);
    capture_initial_content(state, payload, Some(extract_output_text(response)));
}

fn apply_failed_event(state: &mut StreamState, event: &Value, event_type: &str) {
    state.terminal_event_seen = true;
    state.terminal_event_type = Some(event_type.to_string());
    state.finish_reason = Some("failed".to_string());
    if let Some(response) = event.get("response") {
        state.response_obj = Some(response.clone());
        capture_response_lifecycle(state, response);
        capture_provider_error(state, response.get("error"));
    }
    capture_provider_error(state, event.get("error"));
}

fn apply_untyped_event(state: &mut StreamState, event: &Value, payload: &mut StreamPayload) {
    if state.response_obj.is_none()
        && ["output", "output_text", "text", "status", "error"]
            .iter()
            .any(|field| event.get(field).is_some())
    {
        state.response_obj = Some(event.clone());
    }
    capture_initial_content(state, payload, Some(extract_output_text(event)));
    if state.finish_reason.is_none() {
        state.finish_reason = event
            .get("status")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|status| !status.is_empty())
            .map(ToOwned::to_owned);
    }
    capture_response_lifecycle(state, event);
    if let Some(status @ ("completed" | "incomplete" | "failed")) = state.response_status.as_deref()
    {
        state.terminal_event_seen = true;
        state.terminal_event_type = Some(format!("response.{status}"));
    }
}

fn capture_initial_content(
    state: &mut StreamState,
    payload: &mut StreamPayload,
    content: Option<String>,
) {
    if !state.full_content.is_empty() {
        return;
    }
    let Some(content) = content.filter(|content| !content.is_empty()) else {
        return;
    };
    append_stream_text(&mut state.full_content, content.as_str());
    state.sent_any_chunk = true;
    payload.chunk = Some(content);
}

fn capture_response_candidate(state: &mut StreamState, event: &Value) {
    if state.response_obj.is_some() {
        return;
    }
    if let Some(response) = event.get("response") {
        if ["output", "output_text", "status"]
            .iter()
            .any(|field| response.get(field).is_some())
        {
            state.response_obj = Some(response.clone());
        }
    } else if event.get("output").is_some() || event.get("output_text").is_some() {
        state.response_obj = Some(event.clone());
    }
}

fn capture_event_metadata(state: &mut StreamState, event: &Value) {
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
        let error = event
            .get("response")
            .and_then(|response| response.get("error"))
            .or_else(|| event.get("error"));
        capture_provider_error(state, error);
    }
}

fn capture_provider_error(state: &mut StreamState, error: Option<&Value>) {
    if let Some(error) = error.filter(|error| !error.is_null()) {
        state.provider_error = Some(error.clone());
    }
}

fn capture_response_lifecycle(state: &mut StreamState, response: &Value) {
    if let Some(status) = response
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        state.response_status = Some(status.to_string());
        state.finish_reason = Some(status.to_string());
    }
    state.incomplete_details = response
        .get("incomplete_details")
        .cloned()
        .filter(|value| !value.is_null());
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
    if state.response_status.is_none() {
        state.response_status = response_val
            .get("status")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
    if state.incomplete_details.is_none() {
        state.incomplete_details = response_val
            .get("incomplete_details")
            .cloned()
            .filter(|value| !value.is_null());
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
        response_output_items: response_val
            .get("output")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        response_status: state.response_status.clone(),
        incomplete_details: state.incomplete_details.clone(),
        terminal_event_type: state.terminal_event_type.clone(),
        terminal_event_seen: state.terminal_event_seen,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{apply_responses_stream_event, StreamState};

    #[test]
    fn responses_text_deltas_keep_repeated_boundary_characters() {
        let mut state = StreamState::default();

        apply_responses_stream_event(
            &mut state,
            &json!({"type": "response.output_text.delta", "delta": "a"}),
        );
        apply_responses_stream_event(
            &mut state,
            &json!({"type": "response.output_text.delta", "delta": "a"}),
        );

        assert_eq!(state.full_content, "aa");
    }
}
