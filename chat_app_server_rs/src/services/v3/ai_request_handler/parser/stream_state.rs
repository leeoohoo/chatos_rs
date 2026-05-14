use serde_json::Value;

use crate::core::tool_call::join_stream_text;

use super::extractors::{
    extract_output_text, extract_reasoning_event_text, extract_text_delta,
    extract_text_from_fields, looks_like_response_id,
};
use super::tool_calls::{
    ingest_tool_call_item, ingest_tool_calls_from_response_output,
    merge_function_call_arguments_delta, merge_function_call_done,
};
use super::{StreamCallbacksPayload, StreamState};

pub(in crate::services::v3::ai_request_handler) fn apply_stream_event(
    state: &mut StreamState,
    event: &Value,
) -> StreamCallbacksPayload {
    let mut payload = StreamCallbacksPayload::default();

    if let Some(event_type) = event.get("type").and_then(|value| value.as_str()) {
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
            let name_piece = event.get("name").and_then(|value| value.as_str());
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
            if let Some(status) = event.get("status").and_then(|value| value.as_str()) {
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
        .and_then(|value| value.as_str())
    {
        state.response_id = Some(id.to_string());
    } else if state.response_id.is_none() {
        if let Some(id) = event.get("id").and_then(|value| value.as_str()) {
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
