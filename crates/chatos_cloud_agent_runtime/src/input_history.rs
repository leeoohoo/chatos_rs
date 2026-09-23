// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(crate) fn merge_terminal_outcome_overlay(base: Option<Value>, overlay: Value) -> Value {
    match (base, overlay) {
        (Some(Value::Object(mut base)), Value::Object(overlay)) => {
            base.extend(overlay);
            Value::Object(base)
        }
        (_, overlay) => overlay,
    }
}

pub(crate) fn append_response_output_items(
    request_input_items: &[Value],
    response_output_items: &[Value],
    fallback_tool_calls: Option<&Value>,
) -> Vec<Value> {
    // Tool-returned images are injected as transient user messages immediately
    // after their function_call_output so the next model step can inspect them.
    // Once that step has completed, carrying those base64 images into every later
    // stateless Responses request causes unbounded durable-history growth.
    let mut items = prune_consumed_transient_tool_images(request_input_items);
    if response_output_items.is_empty() {
        if let Some(calls) = fallback_tool_calls.and_then(Value::as_array) {
            items.extend(calls.iter().filter_map(|call| {
                let call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)?;
                let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)?;
                let arguments = chatos_ai_runtime::tool_call::tool_call_arguments_text(call);
                Some(chatos_ai_runtime::tool_call::build_function_call_item(
                    call_id,
                    name,
                    arguments.as_str(),
                ))
            }));
        }
    } else {
        items.extend_from_slice(response_output_items);
    }
    prune_items_before_latest_compaction(items)
}

fn prune_consumed_transient_tool_images(items: &[Value]) -> Vec<Value> {
    let mut follows_tool_output = false;
    items
        .iter()
        .filter_map(|item| {
            match item.get("type").and_then(Value::as_str) {
                Some("function_call_output") => {
                    follows_tool_output = true;
                }
                Some("message") if is_image_only_user_message(item) && follows_tool_output => {
                    return None;
                }
                _ => {
                    follows_tool_output = false;
                }
            }
            Some(item.clone())
        })
        .collect()
}

fn is_image_only_user_message(item: &Value) -> bool {
    if item.get("role").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let Some(content) = item.get("content").and_then(Value::as_array) else {
        return false;
    };
    !content.is_empty()
        && content.iter().all(|part| {
            matches!(
                part.get("type").and_then(Value::as_str),
                Some("input_image" | "image_url")
            )
        })
}

/// OpenAI's stateless Responses protocol allows all items preceding the most
/// recent encrypted compaction item to be discarded. Keeping them would defeat
/// compaction and recreate quadratic durable-history growth.
fn prune_items_before_latest_compaction(mut items: Vec<Value>) -> Vec<Value> {
    if let Some(index) = items
        .iter()
        .rposition(|item| item.get("type").and_then(Value::as_str) == Some("compaction"))
    {
        if index > 0 {
            items.drain(..index);
        }
    }
    items
}

pub(crate) fn append_continuation_items(
    request_input_items: &[Value],
    response_output_items: &[Value],
    continuation_items: &[Value],
) -> Vec<Value> {
    let mut items = append_response_output_items(request_input_items, response_output_items, None);
    items.extend_from_slice(continuation_items);
    items
}

pub(crate) fn accumulate_usage(current: &Value, usage: Option<&Value>) -> Value {
    let mut input_tokens = current
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let mut cached_tokens = current
        .get("cached_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let mut output_tokens = current
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let mut requests = current
        .get("requests")
        .and_then(Value::as_u64)
        .unwrap_or_default();

    if let Some(usage) = usage {
        let snapshot = chatos_ai_runtime::extract_usage_snapshot(usage);
        input_tokens = input_tokens.saturating_add(snapshot.input_tokens.max(0));
        cached_tokens = cached_tokens.saturating_add(snapshot.cached_tokens.max(0));
        output_tokens = output_tokens.saturating_add(snapshot.output_tokens.max(0));
        requests = requests.saturating_add(1);
    }

    serde_json::json!({
        "input_tokens": input_tokens,
        "cached_tokens": cached_tokens,
        "output_tokens": output_tokens,
        "requests": requests,
    })
}
