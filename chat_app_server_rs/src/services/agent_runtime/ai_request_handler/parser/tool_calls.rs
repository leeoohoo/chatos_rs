use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::core::tool_call::{
    build_function_tool_call, clone_tool_call_arguments, collect_ordered_tool_calls,
    extract_tool_call_id, merge_indexed_tool_call_parts, remember_tool_call_index,
    resolve_tool_call_index,
};

use super::StreamState;

pub(in crate::services::agent_runtime::ai_request_handler) fn extract_tool_calls(
    response: &Value,
) -> Option<Value> {
    let mut tool_calls: Vec<Value> = Vec::new();

    if let Some(items) = response.get("output").and_then(|value| value.as_array()) {
        for item in items {
            if item.get("type").and_then(|value| value.as_str()) != Some("function_call") {
                continue;
            }

            let call_id = extract_tool_call_id(item).unwrap_or("");
            if call_id.is_empty() {
                continue;
            }

            let name = item
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = clone_tool_call_arguments(item);
            let args_str = arguments
                .as_str()
                .map(|raw| raw.to_string())
                .unwrap_or_else(|| arguments.to_string());

            tool_calls.push(build_function_tool_call(
                call_id,
                name.as_str(),
                args_str.as_str(),
            ));
        }
    }

    if tool_calls.is_empty() {
        None
    } else {
        Some(Value::Array(tool_calls))
    }
}

pub(in crate::services::agent_runtime::ai_request_handler) fn collect_stream_tool_calls(
    tool_calls_map: &BTreeMap<usize, Value>,
) -> Option<Value> {
    collect_ordered_tool_calls(tool_calls_map)
}

pub(super) fn ingest_tool_call_item(
    state: &mut StreamState,
    event: &Value,
    item: &Value,
    extra_arguments_piece: Option<&str>,
) {
    let item_type = item
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if item_type != "function_call" {
        return;
    }

    let item_id = item
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let call_id = item
        .get("call_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let index = resolve_tool_call_index(event, Some(item), &state.tool_call_index_map)
        .unwrap_or_else(|| state.tool_calls_map.len());
    remember_tool_call_index(&mut state.tool_call_index_map, index, item_id, call_id);
    let item_arguments_piece = item.get("arguments").map(|value| {
        value
            .as_str()
            .map(|raw| raw.to_string())
            .unwrap_or_else(|| value.to_string())
    });
    merge_indexed_tool_call_parts(
        &mut state.tool_calls_map,
        index,
        item_id,
        call_id,
        item.get("name").and_then(|value| value.as_str()),
        item_arguments_piece.as_deref().or(extra_arguments_piece),
    );
    if item_arguments_piece.is_some() && extra_arguments_piece.is_some() {
        merge_indexed_tool_call_parts(
            &mut state.tool_calls_map,
            index,
            item_id,
            call_id,
            None,
            extra_arguments_piece,
        );
    }
}

pub(super) fn ingest_tool_calls_from_response_output(state: &mut StreamState, response: &Value) {
    if let Some(items) = response.get("output").and_then(|value| value.as_array()) {
        for (fallback_index, item) in items.iter().enumerate() {
            let mut event = json!({});
            if let Some(output_index) = item.get("output_index").and_then(|value| value.as_u64()) {
                event["output_index"] = json!(output_index);
            } else {
                event["output_index"] = json!(fallback_index as u64);
            }
            ingest_tool_call_item(state, &event, item, None);
        }
    }
}

pub(super) fn merge_function_call_arguments_delta(
    state: &mut StreamState,
    event: &Value,
    arguments_piece: &str,
) {
    let call_id = event
        .get("call_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let item_id = event
        .get("item_id")
        .or_else(|| event.get("id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let index = resolve_tool_call_index(event, None, &state.tool_call_index_map)
        .unwrap_or_else(|| state.tool_calls_map.len());
    remember_tool_call_index(&mut state.tool_call_index_map, index, item_id, call_id);
    merge_indexed_tool_call_parts(
        &mut state.tool_calls_map,
        index,
        item_id,
        call_id,
        None,
        Some(arguments_piece),
    );
}

pub(super) fn merge_function_call_done(
    state: &mut StreamState,
    event: &Value,
    name_piece: Option<&str>,
    arguments_piece: Option<&str>,
) {
    let call_id = event
        .get("call_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let item_id = event
        .get("item_id")
        .or_else(|| event.get("id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let index = resolve_tool_call_index(event, None, &state.tool_call_index_map)
        .unwrap_or_else(|| state.tool_calls_map.len());
    remember_tool_call_index(&mut state.tool_call_index_map, index, item_id, call_id);
    merge_indexed_tool_call_parts(
        &mut state.tool_calls_map,
        index,
        item_id,
        call_id,
        name_piece,
        arguments_piece,
    );
}
