// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chatos_project_execution::{
    STATUS_ALREADY_CONFIRMED, STATUS_AWAITING_CONFIRMATION, STATUS_EXECUTION_STARTED, STATUS_PAUSED,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    models::{TaskRunRecord, TaskRunStatus, TaskStatus},
    services::{
        ChatosMessageModelConfigSummary, ChatosMessageRunDetail, ChatosMessageTaskRun,
        ChatosMessageTaskRunEvent, ChatosMessageTaskSummary,
    },
    state::AppState,
};

use super::internal_auth::{
    require_task_runner_internal_request, TaskRunnerInternalAuditGuard,
    TaskRunnerInternalRequestIdentity, CHATOS_CALLER, CHATOS_EXECUTION_START_SCOPE,
    CHATOS_MESSAGES_READ_SCOPE,
};

const DEFAULT_RUN_EVENT_LIMIT: usize = 40;
const MAX_RUN_EVENT_LIMIT: usize = 100;
const RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES: usize = 256 * 1024;
const RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES: usize = 32 * 1024;
const RUN_EVENT_MESSAGE_PREVIEW_LIMIT_BYTES: usize = 16 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/chatos/message-tasks",
            get(list_chatos_message_tasks),
        )
        .route(
            "/internal/chatos/message-graph",
            get(get_chatos_message_graph),
        )
        .route(
            "/internal/chatos/message-tasks/{task_id}",
            get(get_chatos_message_task),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}",
            get(get_chatos_message_run),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/retry",
            post(retry_chatos_message_run),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/events/{event_id}",
            get(get_chatos_message_run_event),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/output/changes",
            get(get_chatos_message_run_output_changes),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/output/diff",
            get(get_chatos_message_run_output_diff),
        )
        .route(
            "/internal/chatos/message-graph/runs/{run_id}",
            get(get_chatos_message_graph_run),
        )
        .route(
            "/internal/chatos/session-active-message-tasks",
            post(list_chatos_session_active_message_tasks),
        )
        .route(
            "/internal/chatos/project-execution/confirm",
            post(confirm_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/pause",
            post(pause_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/resume",
            post(resume_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/clone",
            post(clone_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/retire",
            post(retire_chatos_project_execution),
        )
}

#[derive(Debug, Deserialize)]
struct ChatosMessageTaskQuery {
    source_session_id: String,
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RetryChatosMessageRunRequest {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
    retry_instruction: Option<String>,
    execution_service_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatosMessageRunQuery {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
    event_limit: Option<usize>,
    event_offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ChatosMessageRunOutputChangesQuery {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ChatosMessageRunOutputDiffQuery {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
    path: String,
}

#[derive(Debug, Serialize)]
struct ChatosMessageTasksResponse {
    items: Vec<ChatosMessageTaskSummary>,
}

#[derive(Debug, Deserialize)]
struct ChatosSessionActiveMessageTasksRequest {
    source_session_id: String,
    #[serde(default)]
    source_user_message_ids: Vec<String>,
    #[serde(default)]
    source_turn_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ChatosActiveMessageTaskSource {
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
    running_count: usize,
    active_count: usize,
}

#[derive(Debug, Serialize)]
struct ChatosSessionActiveMessageTasksResponse {
    source_session_id: String,
    active_source_user_message_ids: Vec<String>,
    running_source_user_message_ids: Vec<String>,
    items: Vec<ChatosActiveMessageTaskSource>,
}

#[derive(Debug, Deserialize)]
struct ConfirmChatosProjectExecutionRequest {
    project_id: String,
    requirement_id: String,
    source_session_id: String,
    source_user_message_id: String,
}

type MutateChatosProjectExecutionRequest = ConfirmChatosProjectExecutionRequest;

#[derive(Debug, Deserialize)]
struct CloneChatosProjectExecutionRequest {
    project_id: String,
    requirement_id: String,
    old_source_session_id: String,
    old_source_user_message_id: String,
    new_source_session_id: String,
    new_source_user_message_id: String,
}

#[derive(Debug, Deserialize)]
struct RetireChatosProjectExecutionRequest {
    project_id: String,
    requirement_id: String,
    source_session_id: String,
    source_user_message_id: String,
}

#[derive(Debug)]
pub(super) struct InternalApiError {
    status: StatusCode,
    message: String,
}

impl InternalApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for InternalApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn validate_chatos_message_query(
    query: &ChatosMessageTaskQuery,
) -> Result<(&str, Option<&str>, Option<&str>), InternalApiError> {
    let source_session_id = query.source_session_id.trim();
    let source_user_message_id = query
        .source_user_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let source_turn_id = query
        .source_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source_session_id.is_empty()
        || (source_user_message_id.is_none() && source_turn_id.is_none())
    {
        return Err(InternalApiError::bad_request(
            "source_session_id and source_user_message_id or source_turn_id are required",
        ));
    }
    Ok((source_session_id, source_user_message_id, source_turn_id))
}

fn run_event_page(query: &ChatosMessageRunQuery) -> (usize, usize) {
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

fn redact_workspace_paths_internal<T>(state: &AppState, value: T) -> Result<Value, InternalApiError>
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

fn trim_run_for_chatos_detail(
    mut run: crate::models::TaskRunRecord,
) -> crate::models::TaskRunRecord {
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
    let plugin_snapshots = value
        .get("plugin_snapshots")
        .map(project_plugin_snapshot_summaries_for_chatos);
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
    if let Some(plugin_snapshots) = plugin_snapshots {
        root.insert("plugin_snapshots".to_string(), plugin_snapshots);
    }
    projected
}

fn project_plugin_snapshot_summaries_for_chatos(value: &Value) -> Value {
    let Some(snapshots) = value.as_array() else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        snapshots
            .iter()
            .take(50)
            .filter_map(Value::as_object)
            .map(|snapshot| {
                let mut projected = serde_json::Map::new();
                copy_bounded_string_fields(
                    snapshot,
                    &mut projected,
                    &[
                        ("plugin_id", 256),
                        ("release_id", 256),
                        ("version", 64),
                        ("artifact_sha256", 64),
                    ],
                );
                if let Some(components) = snapshot
                    .get("component_snapshots")
                    .and_then(Value::as_array)
                {
                    let components = components
                        .iter()
                        .take(128)
                        .filter_map(Value::as_object)
                        .map(|component| {
                            let mut projected_component = serde_json::Map::new();
                            copy_bounded_string_fields(
                                component,
                                &mut projected_component,
                                &[("component_key", 256), ("content_sha256", 64)],
                            );
                            if let Some(kind) = component.get("kind").and_then(Value::as_str) {
                                projected_component
                                    .insert("kind".to_string(), Value::String(kind.to_string()));
                            }
                            Value::Object(projected_component)
                        })
                        .collect();
                    projected.insert("component_snapshots".to_string(), Value::Array(components));
                }
                Value::Object(projected)
            })
            .collect(),
    )
}

fn trim_event_for_chatos_detail(
    mut event: crate::models::TaskRunEventRecord,
) -> crate::models::TaskRunEventRecord {
    event.message = event.message.map(|message| {
        truncate_text_bytes(message.as_str(), RUN_EVENT_MESSAGE_PREVIEW_LIMIT_BYTES)
            .unwrap_or(message)
    });
    event.payload = event.payload.map(|value| {
        let projected = project_plugin_event_payload_for_chatos(event.event_type.as_str(), value);
        truncate_json_value(projected, RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES)
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
    if let Some(plugin_snapshots) = root
        .get_mut("plugin_snapshots")
        .and_then(Value::as_array_mut)
    {
        for plugin in plugin_snapshots {
            let Some(components) = plugin
                .as_object_mut()
                .and_then(|plugin| plugin.get_mut("component_snapshots"))
                .and_then(Value::as_array_mut)
            else {
                continue;
            };
            for component in components {
                let Some(runtime) = component
                    .as_object_mut()
                    .and_then(|component| component.get_mut("runtime"))
                    .and_then(Value::as_object_mut)
                else {
                    continue;
                };
                replace_plugin_arguments_with_audit(runtime);
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

fn project_plugin_event_payload_for_chatos(event_type: &str, payload: Value) -> Value {
    match event_type {
        "plugin_runtime" => project_plugin_runtime_payload(&payload),
        "plugin_hook_blocked" => project_plugin_hook_blocked_payload(&payload),
        "plugin_ui_ready" => project_plugin_ui_ready_payload(&payload),
        "plugin_artifact_ready" => project_plugin_artifact_ready_payload(&payload),
        _ => payload,
    }
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

fn paginate_run_events(
    events: Vec<crate::models::TaskRunEventRecord>,
    limit: usize,
    offset: usize,
) -> (Vec<ChatosMessageTaskRunEvent>, usize, bool) {
    let total = events.len();
    let items = events
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(trim_event_for_chatos_detail)
        .map(ChatosMessageTaskRunEvent::from)
        .collect::<Vec<_>>();
    let has_more = offset.saturating_add(items.len()) < total;
    (items, total, has_more)
}

async fn list_chatos_message_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let items = state
        .task_service
        .list_message_task_summaries_for_chatos_source(
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?;
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageTasksResponse { items },
    )?))
}

async fn list_chatos_session_active_message_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatosSessionActiveMessageTasksRequest>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let source_session_id = request.source_session_id.trim();
    if source_session_id.is_empty() {
        return Err(InternalApiError::bad_request(
            "source_session_id is required",
        ));
    }
    let items = state
        .task_service
        .list_active_message_task_sources_for_chatos_session(
            source_session_id,
            request.source_user_message_ids.as_slice(),
            request.source_turn_ids.as_slice(),
        )
        .await
        .map_err(InternalApiError::internal)?;
    let active_source_user_message_ids = items
        .iter()
        .filter_map(|item| item.source_user_message_id.clone())
        .collect::<Vec<_>>();
    let running_source_user_message_ids = items
        .iter()
        .filter(|item| item.running_count > 0)
        .filter_map(|item| item.source_user_message_id.clone())
        .collect::<Vec<_>>();
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosSessionActiveMessageTasksResponse {
            source_session_id: source_session_id.to_string(),
            running_source_user_message_ids,
            active_source_user_message_ids,
            items: items
                .into_iter()
                .map(|item| ChatosActiveMessageTaskSource {
                    source_user_message_id: item.source_user_message_id,
                    source_turn_id: item.source_turn_id,
                    running_count: item.running_count,
                    active_count: item.active_count,
                })
                .collect(),
        },
    )?))
}

async fn confirm_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_task_runner_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER],
        CHATOS_EXECUTION_START_SCOPE,
    )
    .map_err(|error| InternalApiError {
        status: error.status,
        message: error.message,
    })?;
    let project_id = request.project_id.trim();
    let requirement_id = request.requirement_id.trim();
    let source_session_id = request.source_session_id.trim();
    let source_user_message_id = request.source_user_message_id.trim();
    if project_id.is_empty()
        || requirement_id.is_empty()
        || source_session_id.is_empty()
        || source_user_message_id.is_empty()
    {
        return Err(InternalApiError::bad_request(
            "project_id, requirement_id, source_session_id and source_user_message_id are required",
        ));
    }
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id),
        "project_execution",
        requirement_id,
        "confirm",
    );

    let tasks = state
        .task_service
        .list_tasks_for_chatos_source(source_session_id, Some(source_user_message_id), None)
        .await
        .map_err(InternalApiError::internal)?;
    if tasks.is_empty() {
        return Err(InternalApiError::not_found(
            "project execution task graph is not ready",
        ));
    }
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    for task in &tasks {
        let payload = task.input_payload.as_ref();
        let payload_source = payload
            .and_then(|value| value.get("source"))
            .and_then(Value::as_str);
        let payload_requirement_id = payload
            .and_then(|value| value.get("root_requirement_id"))
            .or_else(|| payload.and_then(|value| value.get("requirement_id")))
            .and_then(Value::as_str);
        if task.project_id.trim() != project_id
            || payload_source != Some("chatos_project_requirement_execution")
            || payload_requirement_id != Some(requirement_id)
        {
            return Err(InternalApiError::conflict(
                "task graph does not belong to the requested project requirement execution",
            ));
        }
        if matches!(
            task.status,
            TaskStatus::Failed | TaskStatus::Blocked | TaskStatus::Cancelled | TaskStatus::Archived
        ) {
            return Err(InternalApiError::conflict(
                "project execution task graph contains failed or cancelled tasks",
            ));
        }
    }

    let root_task_ids = tasks
        .iter()
        .filter(|task| task.prerequisite_task_ids.is_empty())
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    if root_task_ids.is_empty() {
        return Err(InternalApiError::conflict(
            "project execution task graph has no runnable roots",
        ));
    }
    let already_confirmed = tasks
        .iter()
        .any(|task| task.last_run_id.is_some() || task.status != TaskStatus::Ready);
    if !already_confirmed {
        for task in tasks
            .iter()
            .filter(|task| task.mcp_config.requires_execution)
        {
            state
                .run_service
                .validate_sandbox_route_for_task(task)
                .await
                .map_err(|error| {
                    InternalApiError::conflict(format!(
                        "project sandbox environment must be ready before execution: {error}"
                    ))
                })?;
        }
    }
    let started_runs = if already_confirmed {
        let mut existing_runs = Vec::new();
        for run_id in tasks.iter().filter_map(|task| task.last_run_id.as_deref()) {
            if let Some(run) = state
                .run_service
                .get_run(run_id)
                .await
                .map_err(InternalApiError::internal)?
            {
                existing_runs.push(run);
            }
        }
        existing_runs
    } else {
        state
            .run_service
            .dispatch_confirmed_project_execution_tasks(tasks.as_slice())
            .await
            .map_err(InternalApiError::internal)?
    };
    let task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
    let response = Json(redact_workspace_paths_internal(
        &state,
        json!({
            "success": true,
            "status": if already_confirmed { STATUS_ALREADY_CONFIRMED } else { STATUS_EXECUTION_STARTED },
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_session_id": source_session_id,
            "source_user_message_id": source_user_message_id,
            "task_ids": task_ids,
            "root_task_ids": root_task_ids,
            "started_runs": started_runs,
        }),
    )?);
    audit.succeeded();
    Ok(response)
}

async fn pause_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MutateChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    mutate_chatos_project_execution_pause(state, headers, request, true).await
}

async fn resume_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MutateChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    mutate_chatos_project_execution_pause(state, headers, request, false).await
}

async fn mutate_chatos_project_execution_pause(
    state: AppState,
    headers: HeaderMap,
    request: MutateChatosProjectExecutionRequest,
    paused: bool,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let source_session_id = required_internal_text(request.source_session_id, "source_session_id")?;
    let source_user_message_id =
        required_internal_text(request.source_user_message_id, "source_user_message_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id.as_str()),
        "project_execution",
        requirement_id.as_str(),
        if paused { "pause" } else { "resume" },
    );
    let tasks = state
        .task_service
        .list_tasks_for_chatos_source(
            source_session_id.as_str(),
            Some(source_user_message_id.as_str()),
            None,
        )
        .await
        .map_err(InternalApiError::internal)?;
    if tasks.is_empty() {
        return Err(InternalApiError::not_found(
            "project execution task graph is not ready",
        ));
    }
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    for task in &tasks {
        validate_project_execution_task(task, project_id.as_str(), requirement_id.as_str())?;
    }
    if !tasks
        .iter()
        .any(|task| task.last_run_id.is_some() || task.status != TaskStatus::Ready)
    {
        return Err(InternalApiError::conflict(
            "project execution has not started yet",
        ));
    }
    let started_runs = state
        .run_service
        .set_project_execution_paused(tasks.as_slice(), paused)
        .await
        .map_err(InternalApiError::internal)?;
    let task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
    let mut running_count = 0usize;
    let mut queued_count = 0usize;
    for task_id in &task_ids {
        for run in state
            .run_service
            .list_runs(Some(task_id.as_str()))
            .await
            .map_err(InternalApiError::internal)?
        {
            match run.status {
                TaskRunStatus::Running => running_count += 1,
                TaskRunStatus::Queued => queued_count += 1,
                _ => {}
            }
        }
    }
    let response = Json(redact_workspace_paths_internal(
        &state,
        json!({
            "success": true,
            "status": if paused { STATUS_PAUSED } else { STATUS_EXECUTION_STARTED },
            "execution_paused": paused,
            "pause_scope": "future_dispatch",
            "active_runs_continue": true,
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_session_id": source_session_id,
            "source_user_message_id": source_user_message_id,
            "task_ids": task_ids,
            "running_count": running_count,
            "queued_count": queued_count,
            "started_runs": started_runs,
        }),
    )?);
    audit.succeeded();
    Ok(response)
}

async fn clone_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CloneChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let old_source_session_id =
        required_internal_text(request.old_source_session_id, "old_source_session_id")?;
    let old_source_user_message_id = required_internal_text(
        request.old_source_user_message_id,
        "old_source_user_message_id",
    )?;
    let new_source_session_id =
        required_internal_text(request.new_source_session_id, "new_source_session_id")?;
    let new_source_user_message_id = required_internal_text(
        request.new_source_user_message_id,
        "new_source_user_message_id",
    )?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id.as_str()),
        "project_execution",
        requirement_id.as_str(),
        "clone",
    );
    let cloned = state
        .task_service
        .clone_stopped_project_execution_tasks(
            project_id.as_str(),
            requirement_id.as_str(),
            old_source_session_id.as_str(),
            old_source_user_message_id.as_str(),
            new_source_session_id.as_str(),
            new_source_user_message_id.as_str(),
        )
        .await
        .map_err(InternalApiError::conflict)?;
    let tasks = cloned
        .iter()
        .map(|item| item.task.clone())
        .collect::<Vec<_>>();
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    let task_mappings = cloned
        .iter()
        .map(|item| {
            json!({
                "old_task_id": item.old_task_id,
                "new_task_id": item.task.id,
                "project_task_id": item.project_task_id,
                "status": "ready",
                "run_id": Value::Null,
            })
        })
        .collect::<Vec<_>>();
    let response = Json(redact_workspace_paths_internal(
        &state,
        json!({
            "success": true,
            "status": STATUS_AWAITING_CONFIRMATION,
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_session_id": new_source_session_id,
            "source_user_message_id": new_source_user_message_id,
            "task_mappings": task_mappings,
            "task_ids": tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
            "root_task_ids": tasks.iter().filter(|task| task.prerequisite_task_ids.is_empty()).map(|task| task.id.clone()).collect::<Vec<_>>(),
            "started_runs": [],
        }),
    )?);
    audit.succeeded();
    Ok(response)
}

async fn retire_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetireChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let source_session_id = required_internal_text(request.source_session_id, "source_session_id")?;
    let source_user_message_id =
        required_internal_text(request.source_user_message_id, "source_user_message_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id.as_str()),
        "project_execution",
        requirement_id.as_str(),
        "retire",
    );
    let tasks = state
        .task_service
        .list_tasks_for_chatos_source(
            source_session_id.as_str(),
            Some(source_user_message_id.as_str()),
            None,
        )
        .await
        .map_err(InternalApiError::internal)?;
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    for task in &tasks {
        validate_project_execution_task(task, project_id.as_str(), requirement_id.as_str())?;
        if state
            .run_service
            .has_active_run_for_task(task.id.as_str())
            .await
            .map_err(InternalApiError::internal)?
        {
            return Err(InternalApiError::conflict(format!(
                "project execution task still has an active run: {}",
                task.id
            )));
        }
    }
    let mut cleaned_artifacts = Vec::new();
    for task in &tasks {
        let runs = state
            .run_service
            .list_runs(Some(task.id.as_str()))
            .await
            .map_err(InternalApiError::internal)?;
        for run in &runs {
            state
                .run_service
                .release_sandboxes_for_terminal_run(run)
                .await
                .map_err(InternalApiError::internal)?;
            cleaned_artifacts.extend(
                state
                    .run_service
                    .cleanup_harness_artifacts_for_run(run)
                    .await
                    .map_err(InternalApiError::internal)?,
            );
        }
    }
    let mut deleted_task_ids = Vec::new();
    for task in &tasks {
        if state
            .task_service
            .delete_task(task.id.as_str())
            .await
            .map_err(InternalApiError::internal)?
        {
            deleted_task_ids.push(task.id.clone());
        }
    }
    let response = Json(json!({
        "success": true,
        "project_id": project_id,
        "requirement_id": requirement_id,
        "source_session_id": source_session_id,
        "source_user_message_id": source_user_message_id,
        "deleted_task_ids": deleted_task_ids,
        "cleaned_artifacts": cleaned_artifacts,
    }));
    audit.succeeded();
    Ok(response)
}

fn enrich_project_execution_audit(
    audit: &mut TaskRunnerInternalAuditGuard,
    tasks: &[crate::models::TaskRecord],
) {
    let represented_user_id = tasks.iter().find_map(|task| {
        task.owner_user_id
            .as_deref()
            .or(task.creator_user_id.as_deref())
    });
    audit.represented_user_id(represented_user_id);
    audit.tenant_id(tasks.first().map(|task| task.tenant_id.as_str()));
}

fn required_internal_text(value: String, field: &str) -> Result<String, InternalApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(InternalApiError::bad_request(format!(
            "{field} is required"
        )));
    }
    Ok(value.to_string())
}

fn require_chatos_execution_mutation(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TaskRunnerInternalRequestIdentity, InternalApiError> {
    require_task_runner_internal_request(
        &state.config,
        headers,
        &[CHATOS_CALLER],
        CHATOS_EXECUTION_START_SCOPE,
    )
    .map_err(|error| InternalApiError {
        status: error.status,
        message: error.message,
    })
}

fn validate_project_execution_task(
    task: &crate::models::TaskRecord,
    project_id: &str,
    requirement_id: &str,
) -> Result<(), InternalApiError> {
    let payload = task.input_payload.as_ref();
    let payload_source = payload
        .and_then(|value| value.get("source"))
        .and_then(Value::as_str);
    let payload_requirement_id = payload
        .and_then(|value| value.get("root_requirement_id"))
        .or_else(|| payload.and_then(|value| value.get("requirement_id")))
        .and_then(Value::as_str);
    if task.project_id.trim() != project_id
        || payload_source != Some("chatos_project_requirement_execution")
        || payload_requirement_id != Some(requirement_id)
    {
        return Err(InternalApiError::conflict(
            "task graph does not belong to the requested project requirement execution",
        ));
    }
    Ok(())
}

async fn get_chatos_message_task(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let detail = state
        .task_service
        .get_message_task_detail_for_chatos_source(
            task_id.trim(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("task not found for message"))?;
    Ok(Json(redact_workspace_paths_internal(&state, detail)?))
}

async fn get_chatos_message_graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let graph = state
        .task_service
        .get_message_task_graph_for_chatos_source(
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?;
    Ok(Json(redact_workspace_paths_internal(&state, graph)?))
}

async fn get_chatos_message_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageRunQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query.source)?;
    let (event_limit, event_offset) = run_event_page(&query);
    let run = state
        .run_service
        .get_run(run_id.trim())
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let task = state
        .task_service
        .get_message_task_detail_for_chatos_source(
            run.task_id.as_str(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let events = state
        .run_service
        .list_run_events(run.id.as_str())
        .await
        .map_err(InternalApiError::internal)?;
    let (events, events_total, events_has_more) =
        paginate_run_events(events, event_limit, event_offset);
    let model_config = state
        .model_config_service
        .get_model_config(run.model_config_id.as_str())
        .await
        .map_err(InternalApiError::internal)?
        .map(ChatosMessageModelConfigSummary::from);
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageRunDetail {
            task,
            run: ChatosMessageTaskRun::from(trim_run_for_chatos_detail(run)),
            model_config,
            events,
            events_total,
            events_limit: event_limit,
            events_offset: event_offset,
            events_has_more,
        },
    )?))
}

async fn retry_chatos_message_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetryChatosMessageRunRequest>,
) -> Result<(StatusCode, Json<Value>), InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let run_id = required_internal_text(run_id, "run_id")?;
    let mut audit =
        TaskRunnerInternalAuditGuard::new(&identity, None, "task_run", run_id.as_str(), "retry");
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&request.source)?;
    let retry_instruction = request
        .retry_instruction
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let execution_service_id = request
        .execution_service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if retry_instruction.is_some_and(|value| value.chars().count() > 4000) {
        return Err(InternalApiError::bad_request(
            "retry_instruction must not exceed 4000 characters",
        ));
    }
    if execution_service_id.is_some_and(|value| value.chars().count() > 255) {
        return Err(InternalApiError::bad_request(
            "execution_service_id must not exceed 255 characters",
        ));
    }
    let run = require_chatos_message_run(
        &state,
        run_id.as_str(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    if let Ok(Some(task)) = state.task_service.get_task(run.task_id.as_str()).await {
        audit.represented_user_id(
            task.owner_user_id
                .as_deref()
                .or(task.creator_user_id.as_deref()),
        );
        audit.tenant_id(Some(task.tenant_id.as_str()));
        audit.project_id(Some(task.project_id.as_str()));
        audit.resource_name(Some(task.title.as_str()));
    }
    require_retryable_message_run(&run.status)?;
    let retried = state
        .run_service
        .retry_run_with_instruction_and_execution_service(
            run.id.as_str(),
            retry_instruction.map(ToOwned::to_owned),
            execution_service_id.map(ToOwned::to_owned),
        )
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let response = (
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "run": ChatosMessageTaskRun::from(retried),
        })),
    );
    audit.succeeded();
    Ok(response)
}

fn require_retryable_message_run(status: &TaskRunStatus) -> Result<(), InternalApiError> {
    if matches!(status, TaskRunStatus::Failed | TaskRunStatus::Blocked) {
        return Ok(());
    }
    Err(InternalApiError::bad_request(
        "only a failed or blocked message task run can be retried",
    ))
}

async fn get_chatos_message_run_event(
    Path((run_id, event_id)): Path<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let run = require_chatos_message_run(
        &state,
        run_id.trim(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    let event_id = event_id.trim();
    if event_id.is_empty() {
        return Err(InternalApiError::bad_request("run event id is required"));
    }
    let event = state
        .run_service
        .list_run_events(run.id.as_str())
        .await
        .map_err(InternalApiError::internal)?
        .into_iter()
        .find(|event| event.id == event_id && event.run_id == run.id)
        .ok_or_else(|| InternalApiError::not_found("run event not found for message"))?;
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageTaskRunEvent::from(event),
    )?))
}

async fn get_chatos_message_run_output_changes(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageRunOutputChangesQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query.source)?;
    let run = require_chatos_message_run(
        &state,
        run_id.trim(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    let response = state
        .run_service
        .get_run_output_changes(run.id.as_str(), query.limit, query.offset)
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    Ok(Json(redact_workspace_paths_internal(&state, response)?))
}

async fn get_chatos_message_run_output_diff(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageRunOutputDiffQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query.source)?;
    let run = require_chatos_message_run(
        &state,
        run_id.trim(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    let response = state
        .run_service
        .get_run_output_diff(run.id.as_str(), query.path.as_str())
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    Ok(Json(redact_workspace_paths_internal(&state, response)?))
}

async fn require_chatos_message_run(
    state: &AppState,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<TaskRunRecord, InternalApiError> {
    let run = state
        .run_service
        .get_run(run_id)
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    state
        .task_service
        .get_message_task_detail_for_chatos_source(
            run.task_id.as_str(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    Ok(run)
}

async fn get_chatos_message_graph_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageRunQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query.source)?;
    let (event_limit, event_offset) = run_event_page(&query);
    let run = state
        .run_service
        .get_run(run_id.trim())
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for graph"))?;
    let graph = state
        .task_service
        .get_message_task_graph_for_chatos_source(
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?;
    let task = graph
        .nodes
        .into_iter()
        .find(|node| node.task.id == run.task_id)
        .map(|node| node.task)
        .ok_or_else(|| InternalApiError::not_found("run not found for graph"))?;
    let events = state
        .run_service
        .list_run_events(run.id.as_str())
        .await
        .map_err(InternalApiError::internal)?;
    let (events, events_total, events_has_more) =
        paginate_run_events(events, event_limit, event_offset);
    let model_config = state
        .model_config_service
        .get_model_config(run.model_config_id.as_str())
        .await
        .map_err(InternalApiError::internal)?
        .map(ChatosMessageModelConfigSummary::from);
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageRunDetail {
            task,
            run: ChatosMessageTaskRun::from(trim_run_for_chatos_detail(run)),
            model_config,
            events,
            events_total,
            events_limit: event_limit,
            events_offset: event_offset,
            events_has_more,
        },
    )?))
}

fn require_chatos_internal_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TaskRunnerInternalRequestIdentity, InternalApiError> {
    require_task_runner_internal_request(
        &state.config,
        headers,
        &[CHATOS_CALLER],
        CHATOS_MESSAGES_READ_SCOPE,
    )
    .map_err(|err| InternalApiError {
        status: err.status,
        message: err.message,
    })
}

#[cfg(test)]
mod retry_tests {
    use axum::http::StatusCode;

    use super::{require_retryable_message_run, TaskRunStatus};

    #[test]
    fn message_run_retry_accepts_failed_and_blocked_nodes() {
        for status in [TaskRunStatus::Failed, TaskRunStatus::Blocked] {
            require_retryable_message_run(&status)
                .expect("terminal problem run should be retryable");
        }
        for status in [
            TaskRunStatus::Queued,
            TaskRunStatus::Running,
            TaskRunStatus::Succeeded,
            TaskRunStatus::Cancelled,
        ] {
            let error = require_retryable_message_run(&status)
                .expect_err("non-retryable run must not be retried from a message task card");
            assert_eq!(error.status, StatusCode::BAD_REQUEST);
        }
    }
}

#[cfg(test)]
mod plugin_projection_tests {
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    use super::{
        paginate_run_events, trim_run_for_chatos_detail, TaskRunRecord,
        RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES,
    };
    use crate::models::TaskRunEventRecord;

    #[test]
    fn chatos_run_snapshot_replaces_plugin_command_arguments_with_hashes() {
        let command_arguments = "检查 src/private.rs access_token=do-not-display";
        let expected_sha256 = hex::encode(Sha256::digest(command_arguments.as_bytes()));
        let run = TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({
                "plugin_config": {
                    "device_id": "device-1",
                    "workspace_id": "workspace-1",
                    "selected_plugins": [{
                        "plugin_id": "plugin-review",
                        "selected_command_ids": ["review"]
                    }],
                    "command_invocations": [{
                        "plugin_id": "plugin-review",
                        "command_id": "review",
                        "arguments": command_arguments
                    }]
                },
                "plugin_snapshots": [{
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "component_snapshots": [{
                        "component_key": "review",
                        "kind": "command",
                        "runtime": {
                            "runtime_kind": "markdown_command",
                            "arguments": command_arguments
                        }
                    }]
                }]
            }),
            Vec::new(),
            "2026-07-27T00:00:00Z".to_string(),
        );

        let projected = trim_run_for_chatos_detail(run).input_snapshot;
        let serialized = serde_json::to_string(&projected).expect("serialize projected snapshot");

        assert!(!serialized.contains(command_arguments));
        assert!(!serialized.contains("do-not-display"));
        assert_eq!(
            projected.pointer("/plugin_config/command_invocations/0/arguments_present"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            projected
                .pointer("/plugin_config/command_invocations/0/arguments_sha256")
                .and_then(Value::as_str),
            Some(expected_sha256.as_str())
        );
        assert_eq!(
            projected
                .pointer("/plugin_snapshots/0/component_snapshots/0/runtime/arguments_sha256")
                .and_then(Value::as_str),
            Some(expected_sha256.as_str())
        );
    }

    #[test]
    fn oversized_chatos_run_snapshot_retains_bounded_plugin_audit_summary() {
        let run = TaskRunRecord::queued(
            "run-oversized".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({
                "padding": "x".repeat(RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES + 1024),
                "plugin_config": {
                    "device_id": "device-1",
                    "selected_plugins": [{
                        "plugin_id": "plugin-review",
                        "selected_command_ids": ["review"]
                    }],
                    "command_invocations": [{
                        "plugin_id": "plugin-review",
                        "command_id": "review",
                        "arguments": "private-command-arguments"
                    }]
                },
                "plugin_snapshots": [{
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "version": "1.2.3",
                    "device_id": "must-not-be-copied-to-summary",
                    "component_snapshots": [{
                        "component_key": "review",
                        "kind": "command",
                        "content_sha256": "a".repeat(64),
                        "runtime": {"arguments": "private-command-arguments"}
                    }]
                }]
            }),
            Vec::new(),
            "2026-07-27T00:00:00Z".to_string(),
        );

        let projected = trim_run_for_chatos_detail(run).input_snapshot;
        let serialized = serde_json::to_string(&projected).expect("serialize projected snapshot");

        assert_eq!(
            projected.get("truncated").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            projected
                .pointer("/plugin_config/selected_plugins/0/plugin_id")
                .and_then(Value::as_str),
            Some("plugin-review")
        );
        assert_eq!(
            projected
                .pointer("/plugin_snapshots/0/component_snapshots/0/component_key")
                .and_then(Value::as_str),
            Some("review")
        );
        assert!(!serialized.contains("private-command-arguments"));
        assert!(!serialized.contains("must-not-be-copied-to-summary"));
    }

    #[test]
    fn chatos_plugin_events_are_projected_before_diagnostic_display() {
        let secret = "must-not-reach-chatos-plugin-display";
        let events = vec![
            event(
                "event-runtime",
                "plugin_runtime",
                json!({
                    "run_id": "run-1",
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "component_key": "review-hooks",
                    "adapter_session_id": "adapter-1",
                    "phase": "execute",
                    "status": "failed",
                    "operation": "dispatch_hook_event",
                    "tool_name": "browser_snapshot",
                    "duration_ms": 25,
                    "error": "approval declined",
                    "arguments": secret,
                    "tool_payload": {"content": secret},
                    "stdout": secret,
                    "stderr": secret,
                    "hook_dispatch": {
                        "event": "PreToolUse",
                        "snapshot_sha256": "a".repeat(64),
                        "blocking_failure": false,
                        "executions": [{
                            "hook_id": "private-hook-id",
                            "matched": true,
                            "succeeded": false,
                            "timed_out": false,
                            "workspace_write": true,
                            "workspace_write_approved": false,
                            "stdout_sha256": "b".repeat(64),
                            "stderr_sha256": "c".repeat(64),
                            "error": secret
                        }]
                    }
                }),
            ),
            event(
                "event-hook",
                "plugin_hook_blocked",
                json!({
                    "event": "PreToolUse",
                    "blocking_failure": true,
                    "tool_name": "browser_snapshot",
                    "tool_kind": "builtin",
                    "component_key": "review-hooks",
                    "summary_sha256": "d".repeat(64),
                    "raw_payload": secret
                }),
            ),
            event(
                "event-ui",
                "plugin_ui_ready",
                json!({
                    "event_schema_version": 1,
                    "run_id": "run-1",
                    "device_id": "device-secret",
                    "workspace_id": "workspace-secret",
                    "plugin_id": "plugin-review",
                    "release_id": "release-1",
                    "artifact_sha256": "e".repeat(64),
                    "component_key": "workbench",
                    "adapter_session_id": "adapter-ui",
                    "ui": {
                        "title": "Review Workbench",
                        "surface": "workbench",
                        "snapshot_sha256": "f".repeat(64),
                        "bridge_protocol_version": 1,
                        "bridge_capabilities": ["host.context.read", "artifact.list"],
                        "artifact_mime_types": ["application/json"],
                        "relative_source_path": secret,
                        "assets": [{"relative_path": secret}],
                        "content_security_policy": secret
                    }
                }),
            ),
            event(
                "event-artifact",
                "plugin_artifact_ready",
                json!({
                    "event_schema_version": 1,
                    "artifact": {
                        "artifact_id": format!("pa_{}", "1".repeat(32)),
                        "owner": {
                            "owner_user_id": "owner-secret",
                            "run_id": "run-1",
                            "device_id": "device-secret",
                            "workspace_id": "workspace-secret",
                            "plugin_id": "plugin-review",
                            "release_id": "release-1",
                            "artifact_sha256": "e".repeat(64),
                            "component_key": "documents",
                            "adapter_session_id": "adapter-documents"
                        },
                        "workspace_relative_path": secret,
                        "display_name": "report.docx",
                        "media_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                        "size_bytes": 42,
                        "sha256": "2".repeat(64),
                        "created_at": "2026-07-27T00:00:00Z",
                        "producer_tool_name": "create_document",
                        "downloadable": true,
                        "mutable": false,
                        "body_base64": secret
                    }
                }),
            ),
        ];

        let (events, total, has_more) = paginate_run_events(events, 10, 0);
        let serialized = serde_json::to_string(&events).expect("serialize projected events");

        assert_eq!(total, 4);
        assert!(!has_more);
        assert!(serialized.len() < RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES);
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains("owner-secret"));
        assert!(!serialized.contains("device-secret"));
        assert!(!serialized.contains("workspace-secret"));
        assert!(!serialized.contains("stdout_sha256"));
        assert!(!serialized.contains("stderr_sha256"));
        assert!(!serialized.contains("body_base64"));
        assert_eq!(
            events[0]
                .payload
                .as_ref()
                .and_then(|payload| payload.pointer("/hook_dispatch/executions/0/matched")),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            events[2]
                .payload
                .as_ref()
                .and_then(|payload| payload.pointer("/ui/title"))
                .and_then(Value::as_str),
            Some("Review Workbench")
        );
        assert_eq!(
            events[3]
                .payload
                .as_ref()
                .and_then(|payload| payload.pointer("/artifact/display_name"))
                .and_then(Value::as_str),
            Some("report.docx")
        );
    }

    fn event(id: &str, event_type: &str, payload: Value) -> TaskRunEventRecord {
        TaskRunEventRecord {
            id: id.to_string(),
            run_id: "run-1".to_string(),
            event_type: event_type.to_string(),
            message: Some(format!("{event_type} event")),
            payload: Some(payload),
            created_at: "2026-07-27T00:00:00Z".to_string(),
        }
    }
}
