// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::*;

pub(super) fn run_event_page(query: &ChatosMessageRunQuery) -> (usize, usize) {
    (
        query
            .event_limit
            .unwrap_or(DEFAULT_RUN_EVENT_LIMIT)
            .clamp(1, MAX_RUN_EVENT_LIMIT),
        query.event_offset.unwrap_or(0),
    )
}

fn truncate_text_bytes(value: &str, max_bytes: usize) -> Option<String> {
    if value.len() <= max_bytes {
        return None;
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    Some(format!(
        "{}\n\n...（内容已截断，原始大小 {} bytes）",
        &value[..end],
        value.len()
    ))
}

fn truncate_text_chars(value: &str, max_chars: usize) -> Option<String> {
    let original_chars = value.chars().count();
    if original_chars <= max_chars {
        return None;
    }
    let preview = value.chars().take(max_chars).collect::<String>();
    Some(format!(
        "{}\n\n...（内容已截断，原始大小 {} chars）",
        preview, original_chars
    ))
}

fn preview_json_value(value: &Value, max_bytes: usize) -> String {
    let text = value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| serde_json::to_string_pretty(value).ok())
        .unwrap_or_else(|| value.to_string());
    truncate_text_bytes(text.as_str(), max_bytes).unwrap_or(text)
}

fn truncate_json_value(value: Value, max_bytes: usize) -> Value {
    let Ok(bytes) = serde_json::to_vec(&value) else {
        return value;
    };
    if bytes.len() <= max_bytes {
        return value;
    }
    json!({
        "truncated": true,
        "original_bytes": bytes.len(),
        "preview": preview_json_value(&value, max_bytes),
    })
}

fn truncate_json_value_chars(value: Value, max_chars: usize) -> Value {
    let Ok(compact) = serde_json::to_string(&value) else {
        return value;
    };
    let original_bytes = compact.len();
    let original_chars = compact.chars().count();
    if original_chars <= max_chars {
        return value;
    }
    let preview = value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| serde_json::to_string_pretty(&value).ok())
        .unwrap_or(compact);
    json!({
        "truncated": true,
        "original_chars": original_chars,
        "original_bytes": original_bytes,
        "preview": truncate_text_chars(preview.as_str(), max_chars).unwrap_or(preview),
    })
}

pub(super) fn redact_workspace_paths_internal<T>(
    state: &AppState,
    value: T,
) -> Result<Value, InternalApiError>
where
    T: Serialize,
{
    let redactor = crate::services::path_redaction::WorkspacePathRedactor::for_workspace_base(
        state.config.default_workspace_dir.as_str(),
    );
    let mut json =
        serde_json::to_value(value).map_err(|err| InternalApiError::internal(err.to_string()))?;
    redactor.redact_value(&mut json);
    Ok(json)
}

pub(super) fn trim_run_for_chatos_detail(
    mut run: crate::models::TaskRunRecord,
) -> crate::models::TaskRunRecord {
    run.agent_ordering_lane_key = None;
    run.execution_lane_key = None;
    run.chatos_callback_delivery = None;
    redact_plugin_command_arguments(&mut run.input_snapshot);
    run.input_snapshot = truncate_run_input_snapshot_for_chatos(run.input_snapshot);
    run.context_snapshot = run
        .context_snapshot
        .map(|value| truncate_json_value(value, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES));
    run.report = run
        .report
        .map(|value| truncate_json_value(value, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES));
    run
}

fn truncate_run_input_snapshot_for_chatos(value: Value) -> Value {
    let plugin_config = value.get("plugin_config").cloned();
    let mut projected = truncate_json_value(value, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES);
    if projected.get("truncated").and_then(Value::as_bool) != Some(true) {
        return projected;
    }
    let Some(root) = projected.as_object_mut() else {
        return projected;
    };
    if let Some(plugin_config) = plugin_config {
        root.insert("plugin_config".to_string(), plugin_config);
    }
    projected
}

pub(super) fn trim_event_for_chatos_detail(
    mut event: crate::models::TaskRunEventRecord,
    tool_text_limit_chars: usize,
) -> crate::models::TaskRunEventRecord {
    event.message = event.message.map(|message| {
        truncate_text_bytes(message.as_str(), RUN_EVENT_MESSAGE_PREVIEW_LIMIT_BYTES)
            .unwrap_or(message)
    });
    event.payload = event.payload.map(|value| {
        let projected = project_plugin_event_payload_for_chatos(
            event.event_type.as_str(),
            value,
            tool_text_limit_chars,
        );
        if matches!(event.event_type.as_str(), "tool_stream" | "tools_start") {
            projected
        } else {
            truncate_json_value(projected, RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES)
        }
    });
    event
}

fn redact_plugin_command_arguments(snapshot: &mut Value) {
    let Some(root) = snapshot.as_object_mut() else {
        return;
    };
    if let Some(command_invocations) = root
        .get_mut("plugin_config")
        .and_then(Value::as_object_mut)
        .and_then(|config| config.get_mut("command_invocations"))
        .and_then(Value::as_array_mut)
    {
        for invocation in command_invocations {
            if let Some(invocation) = invocation.as_object_mut() {
                replace_plugin_arguments_with_audit(invocation);
            }
        }
    }
}

fn replace_plugin_arguments_with_audit(object: &mut serde_json::Map<String, Value>) {
    let Some(arguments) = object.remove("arguments") else {
        return;
    };
    if arguments.is_null() {
        object.insert("arguments_present".to_string(), Value::Bool(false));
        return;
    }
    let bytes = match &arguments {
        Value::String(value) => value.as_bytes().to_vec(),
        _ => serde_json::to_vec(&arguments).unwrap_or_default(),
    };
    object.insert("arguments_present".to_string(), Value::Bool(true));
    object.insert(
        "arguments_sha256".to_string(),
        Value::String(hex::encode(Sha256::digest(bytes))),
    );
}

fn project_plugin_event_payload_for_chatos(
    event_type: &str,
    payload: Value,
    tool_text_limit_chars: usize,
) -> Value {
    match event_type {
        "tool_stream" => project_tool_stream_payload_for_chatos(&payload, tool_text_limit_chars),
        "tools_start" => project_tools_start_payload_for_chatos(&payload, tool_text_limit_chars),
        "plugin_runtime" => project_plugin_runtime_payload(&payload),
        "plugin_hook_blocked" => project_plugin_hook_blocked_payload(&payload),
        "plugin_ui_ready" => project_plugin_ui_ready_payload(&payload),
        "plugin_artifact_ready" => project_plugin_artifact_ready_payload(&payload),
        _ => payload,
    }
}

fn project_tool_stream_payload_for_chatos(payload: &Value, tool_text_limit_chars: usize) -> Value {
    let Some(source) = payload.as_object() else {
        return payload.clone();
    };
    let mut projected = serde_json::Map::new();
    copy_bounded_string_fields(
        source,
        &mut projected,
        &[
            ("tool_call_id", 256),
            ("call_id", 256),
            ("id", 256),
            ("name", 512),
            ("conversation_turn_id", 256),
            ("invocation_id", 256),
        ],
    );
    for field in ["success", "is_error", "is_stream", "fatal_error"] {
        copy_bool(source, &mut projected, field);
    }

    if let Some(result) = source.get("result").filter(|value| !value.is_null()) {
        projected.insert(
            "result".to_string(),
            truncate_json_value_chars(result.clone(), tool_text_limit_chars),
        );
    }
    let should_include_content = !projected.contains_key("result")
        || source.get("is_error").and_then(Value::as_bool) == Some(true)
        || source.get("success").and_then(Value::as_bool) == Some(false);
    if should_include_content {
        if let Some(content) = source.get("content").and_then(Value::as_str) {
            projected.insert(
                "content".to_string(),
                Value::String(
                    truncate_text_chars(content, tool_text_limit_chars)
                        .unwrap_or_else(|| content.to_string()),
                ),
            );
        }
    }
    Value::Object(projected)
}

fn project_tools_start_payload_for_chatos(payload: &Value, tool_text_limit_chars: usize) -> Value {
    if let Some(calls) = payload.as_array() {
        return Value::Array(
            calls
                .iter()
                .take(64)
                .map(|call| project_tool_call_for_chatos(call, tool_text_limit_chars))
                .collect(),
        );
    }
    let Some(source) = payload.as_object() else {
        return payload.clone();
    };
    let mut projected = source.clone();
    for field in ["tool_calls", "toolCalls", "calls", "tools"] {
        let Some(calls) = source.get(field).and_then(Value::as_array) else {
            continue;
        };
        projected.insert(
            field.to_string(),
            Value::Array(
                calls
                    .iter()
                    .take(64)
                    .map(|call| project_tool_call_for_chatos(call, tool_text_limit_chars))
                    .collect(),
            ),
        );
    }
    Value::Object(projected)
}

fn project_tool_call_for_chatos(call: &Value, tool_text_limit_chars: usize) -> Value {
    let Some(source) = call.as_object() else {
        return call.clone();
    };
    let mut projected = serde_json::Map::new();
    copy_bounded_string_fields(
        source,
        &mut projected,
        &[
            ("id", 256),
            ("call_id", 256),
            ("tool_call_id", 256),
            ("type", 64),
            ("name", 512),
            ("invocation_id", 256),
        ],
    );
    if let Some(function) = source.get("function").and_then(Value::as_object) {
        let mut projected_function = serde_json::Map::new();
        copy_bounded_string_fields(function, &mut projected_function, &[("name", 512)]);
        if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
            projected_function.insert(
                "arguments".to_string(),
                Value::String(
                    truncate_text_chars(arguments, tool_text_limit_chars)
                        .unwrap_or_else(|| arguments.to_string()),
                ),
            );
        }
        projected.insert("function".to_string(), Value::Object(projected_function));
    } else if let Some(arguments) = source.get("arguments") {
        projected.insert(
            "arguments".to_string(),
            truncate_json_value_chars(arguments.clone(), tool_text_limit_chars),
        );
    }
    Value::Object(projected)
}

fn project_plugin_runtime_payload(payload: &Value) -> Value {
    let Some(source) = payload.as_object() else {
        return json!({});
    };
    let mut projected = serde_json::Map::new();
    copy_bounded_string_fields(
        source,
        &mut projected,
        &[
            ("run_id", 256),
            ("plugin_id", 256),
            ("release_id", 256),
            ("component_key", 256),
            ("adapter_session_id", 256),
            ("phase", 64),
            ("status", 64),
            ("operation", 256),
            ("tool_name", 256),
            ("health_status", 64),
            ("error", 1024),
        ],
    );
    copy_non_negative_number(source, &mut projected, "duration_ms");
    if let Some(hook) = source.get("hook_dispatch") {
        projected.insert(
            "hook_dispatch".to_string(),
            project_plugin_hook_dispatch(hook),
        );
    }
    Value::Object(projected)
}

fn project_plugin_hook_dispatch(payload: &Value) -> Value {
    let Some(source) = payload.as_object() else {
        return json!({});
    };
    let mut projected = serde_json::Map::new();
    copy_bounded_string_fields(
        source,
        &mut projected,
        &[("event", 128), ("snapshot_sha256", 64)],
    );
    copy_bool(source, &mut projected, "blocking_failure");
    if let Some(executions) = source.get("executions").and_then(Value::as_array) {
        let executions = executions
            .iter()
            .take(128)
            .filter_map(Value::as_object)
            .map(|execution| {
                let mut item = serde_json::Map::new();
                for field in [
                    "matched",
                    "succeeded",
                    "timed_out",
                    "workspace_write",
                    "workspace_write_approved",
                ] {
                    copy_bool(execution, &mut item, field);
                }
                Value::Object(item)
            })
            .collect();
        projected.insert("executions".to_string(), Value::Array(executions));
    }
    Value::Object(projected)
}

fn project_plugin_hook_blocked_payload(payload: &Value) -> Value {
    let Some(source) = payload.as_object() else {
        return json!({});
    };
    let mut projected = serde_json::Map::new();
    copy_bounded_string_fields(
        source,
        &mut projected,
        &[
            ("plugin_id", 256),
            ("release_id", 256),
            ("adapter_session_id", 256),
            ("event", 128),
            ("tool_name", 256),
            ("tool_kind", 128),
            ("component_key", 256),
            ("summary_sha256", 64),
            ("error", 1024),
        ],
    );
    copy_bool(source, &mut projected, "blocking_failure");
    Value::Object(projected)
}

fn project_plugin_ui_ready_payload(payload: &Value) -> Value {
    let Some(source) = payload.as_object() else {
        return json!({});
    };
    let mut projected = serde_json::Map::new();
    copy_non_negative_number(source, &mut projected, "event_schema_version");
    copy_bounded_string_fields(
        source,
        &mut projected,
        &[
            ("run_id", 256),
            ("plugin_id", 256),
            ("release_id", 256),
            ("artifact_sha256", 64),
            ("component_key", 256),
            ("adapter_session_id", 256),
        ],
    );
    if let Some(ui) = source.get("ui").and_then(Value::as_object) {
        let mut projected_ui = serde_json::Map::new();
        copy_bounded_string_fields(
            ui,
            &mut projected_ui,
            &[("title", 240), ("surface", 64), ("snapshot_sha256", 64)],
        );
        copy_non_negative_number(ui, &mut projected_ui, "bridge_protocol_version");
        copy_bounded_string_array(ui, &mut projected_ui, "bridge_capabilities", 16, 128);
        copy_bounded_string_array(ui, &mut projected_ui, "artifact_mime_types", 32, 128);
        projected.insert("ui".to_string(), Value::Object(projected_ui));
    }
    Value::Object(projected)
}

fn project_plugin_artifact_ready_payload(payload: &Value) -> Value {
    let Some(source) = payload.as_object() else {
        return json!({});
    };
    let mut projected = serde_json::Map::new();
    copy_non_negative_number(source, &mut projected, "event_schema_version");
    if let Some(artifact) = source.get("artifact").and_then(Value::as_object) {
        let mut projected_artifact = serde_json::Map::new();
        copy_bounded_string_fields(
            artifact,
            &mut projected_artifact,
            &[
                ("artifact_id", 128),
                ("display_name", 512),
                ("media_type", 128),
                ("sha256", 64),
                ("created_at", 64),
                ("producer_tool_name", 256),
            ],
        );
        copy_non_negative_number(artifact, &mut projected_artifact, "size_bytes");
        copy_bool(artifact, &mut projected_artifact, "downloadable");
        copy_bool(artifact, &mut projected_artifact, "mutable");
        if let Some(owner) = artifact.get("owner").and_then(Value::as_object) {
            let mut projected_owner = serde_json::Map::new();
            copy_bounded_string_fields(
                owner,
                &mut projected_owner,
                &[
                    ("run_id", 256),
                    ("plugin_id", 256),
                    ("release_id", 256),
                    ("artifact_sha256", 64),
                    ("component_key", 256),
                    ("adapter_session_id", 256),
                ],
            );
            projected_artifact.insert("owner".to_string(), Value::Object(projected_owner));
        }
        projected.insert("artifact".to_string(), Value::Object(projected_artifact));
    }
    Value::Object(projected)
}

fn copy_bounded_string_fields(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    fields: &[(&str, usize)],
) {
    for (field, max_bytes) in fields {
        let Some(value) = source.get(*field).and_then(Value::as_str) else {
            continue;
        };
        let value = truncate_text_bytes(value, *max_bytes).unwrap_or_else(|| value.to_string());
        target.insert((*field).to_string(), Value::String(value));
    }
}

fn copy_non_negative_number(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    field: &str,
) {
    if let Some(value) = source
        .get(field)
        .and_then(Value::as_u64)
        .map(serde_json::Number::from)
    {
        target.insert(field.to_string(), Value::Number(value));
    }
}

fn copy_bool(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    field: &str,
) {
    if let Some(value) = source.get(field).and_then(Value::as_bool) {
        target.insert(field.to_string(), Value::Bool(value));
    }
}

fn copy_bounded_string_array(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    field: &str,
    max_items: usize,
    max_item_bytes: usize,
) {
    let Some(values) = source.get(field).and_then(Value::as_array) else {
        return;
    };
    let values = values
        .iter()
        .take(max_items)
        .filter_map(Value::as_str)
        .filter(|value| value.len() <= max_item_bytes)
        .map(|value| Value::String(value.to_string()))
        .collect();
    target.insert(field.to_string(), Value::Array(values));
}

pub(super) fn paginate_run_events(
    events: Vec<crate::models::TaskRunEventRecord>,
    limit: usize,
    offset: usize,
    tool_text_limit_chars: usize,
) -> (Vec<ChatosMessageTaskRunEvent>, usize, bool) {
    let total = events.len();
    let items = events
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|event| trim_event_for_chatos_detail(event, tool_text_limit_chars))
        .map(ChatosMessageTaskRunEvent::from)
        .collect::<Vec<_>>();
    let has_more = offset.saturating_add(items.len()) < total;
    (items, total, has_more)
}
