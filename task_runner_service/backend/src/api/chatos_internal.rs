// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    models::TaskRunRecord,
    services::{
        ChatosMessageModelConfigSummary, ChatosMessageRunDetail, ChatosMessageTaskRun,
        ChatosMessageTaskRunEvent, ChatosMessageTaskSummary,
    },
    state::AppState,
};

use super::internal_auth::{
    require_task_runner_internal_request, CHATOS_CALLER, CHATOS_MESSAGES_READ_SCOPE,
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
}

#[derive(Debug, Deserialize)]
struct ChatosMessageTaskQuery {
    source_session_id: String,
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
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
    run.chatos_callback_delivery = None;
    run.input_snapshot = truncate_json_value(run.input_snapshot, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES);
    run.context_snapshot = run
        .context_snapshot
        .map(|value| truncate_json_value(value, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES));
    run.report = run
        .report
        .map(|value| truncate_json_value(value, RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES));
    run
}

fn trim_event_for_chatos_detail(
    mut event: crate::models::TaskRunEventRecord,
) -> crate::models::TaskRunEventRecord {
    event.message = event.message.map(|message| {
        truncate_text_bytes(message.as_str(), RUN_EVENT_MESSAGE_PREVIEW_LIMIT_BYTES)
            .unwrap_or(message)
    });
    event.payload = event
        .payload
        .map(|value| truncate_json_value(value, RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES));
    event
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
) -> Result<(), InternalApiError> {
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
