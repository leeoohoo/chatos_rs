// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(crate) const TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE: &str = "task_runner_async_plan";

pub(crate) fn build_ai_client_success_payload(
    content: String,
    reasoning: Option<String>,
    finish_reason: Option<String>,
    iteration: i64,
) -> Value {
    serde_json::json!({
        "success": true,
        "content": content,
        "reasoning": reasoning,
        "tool_calls": Value::Null,
        "finish_reason": finish_reason,
        "iteration": iteration
    })
}

pub(crate) fn attach_ai_client_success_extra(payload: Value, extra: Value) -> Value {
    let mut base = match payload {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };
    if let Value::Object(extra_map) = extra {
        for (key, value) in extra_map {
            base.insert(key, value);
        }
    }
    Value::Object(base)
}

pub(crate) fn normalize_task_runner_async_plan_metadata(metadata: Option<Value>) -> Option<Value> {
    normalize_task_runner_async_metadata(metadata, "plan_summary")
}

pub(crate) fn normalize_task_runner_async_tool_call_metadata(
    metadata: Option<Value>,
) -> Option<Value> {
    normalize_task_runner_async_metadata(metadata, "tool_call")
}

fn normalize_task_runner_async_metadata(
    metadata: Option<Value>,
    message_kind: &str,
) -> Option<Value> {
    let mut root = match metadata {
        Some(Value::Object(map)) => map,
        Some(_) | None => serde_json::Map::new(),
    };

    let task_runner_async = root
        .entry("task_runner_async".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Value::Object(task_runner_async_map) = task_runner_async else {
        root.insert(
            "task_runner_async".to_string(),
            serde_json::json!({
                "mode": "contact_async",
                "message_kind": message_kind
            }),
        );
        return Some(Value::Object(root));
    };

    task_runner_async_map.insert(
        "mode".to_string(),
        Value::String("contact_async".to_string()),
    );
    task_runner_async_map.insert(
        "message_kind".to_string(),
        Value::String(message_kind.to_string()),
    );
    Some(Value::Object(root))
}

pub(crate) fn build_assistant_message_metadata(
    tool_calls: Option<&Value>,
    response_id: Option<&str>,
    turn_id: Option<&str>,
    response_status: Option<&str>,
    extra_metadata: Option<&Value>,
) -> Option<Value> {
    let mut map = serde_json::Map::new();

    if let Some(turn) = turn_id.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(
            "conversation_turn_id".to_string(),
            Value::String(turn.to_string()),
        );
    }
    if let Some(id) = response_id.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert("response_id".to_string(), Value::String(id.to_string()));
    }
    if let Some(status) = response_status
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        map.insert(
            "response_status".to_string(),
            Value::String(status.to_string()),
        );
    }
    if let Some(tool_calls) = tool_calls {
        map.insert("toolCalls".to_string(), tool_calls.clone());
    }
    if let Some(Value::Object(extra)) = extra_metadata {
        for (key, value) in extra {
            map.insert(key.clone(), value.clone());
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}
