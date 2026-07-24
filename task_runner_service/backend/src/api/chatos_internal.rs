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

use crate::{
    models::{TaskRunRecord, TaskRunStatus, TaskStatus},
    services::{
        ChatosMessageModelConfigSummary, ChatosMessageRunDetail, ChatosMessageTaskRun,
        ChatosMessageTaskRunEvent, ChatosMessageTaskSummary,
    },
    state::AppState,
};

use super::internal_auth::{
    require_task_runner_internal_request, CHATOS_CALLER, CHATOS_EXECUTION_START_SCOPE,
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

async fn confirm_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    require_task_runner_internal_request(
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
    Ok(Json(redact_workspace_paths_internal(
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
    )?))
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
    require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let source_session_id = required_internal_text(request.source_session_id, "source_session_id")?;
    let source_user_message_id =
        required_internal_text(request.source_user_message_id, "source_user_message_id")?;
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
    Ok(Json(redact_workspace_paths_internal(
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
    )?))
}

async fn clone_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CloneChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_execution_mutation(&state, &headers)?;
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
    Ok(Json(redact_workspace_paths_internal(
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
    )?))
}

async fn retire_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetireChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let source_session_id = required_internal_text(request.source_session_id, "source_session_id")?;
    let source_user_message_id =
        required_internal_text(request.source_user_message_id, "source_user_message_id")?;
    let tasks = state
        .task_service
        .list_tasks_for_chatos_source(
            source_session_id.as_str(),
            Some(source_user_message_id.as_str()),
            None,
        )
        .await
        .map_err(InternalApiError::internal)?;
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
    Ok(Json(json!({
        "success": true,
        "project_id": project_id,
        "requirement_id": requirement_id,
        "source_session_id": source_session_id,
        "source_user_message_id": source_user_message_id,
        "deleted_task_ids": deleted_task_ids,
        "cleaned_artifacts": cleaned_artifacts,
    })))
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
) -> Result<(), InternalApiError> {
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
    Json(source): Json<ChatosMessageTaskQuery>,
) -> Result<(StatusCode, Json<Value>), InternalApiError> {
    require_chatos_execution_mutation(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&source)?;
    let run = require_chatos_message_run(
        &state,
        run_id.trim(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    require_failed_message_run(&run.status)?;
    let retried = state
        .run_service
        .retry_run(run.id.as_str())
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "run": ChatosMessageTaskRun::from(retried),
        })),
    ))
}

fn require_failed_message_run(status: &TaskRunStatus) -> Result<(), InternalApiError> {
    if status == &TaskRunStatus::Failed {
        return Ok(());
    }
    Err(InternalApiError::bad_request(
        "only a failed message task run can be retried",
    ))
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

#[cfg(test)]
mod retry_tests {
    use axum::http::StatusCode;

    use super::{require_failed_message_run, TaskRunStatus};

    #[test]
    fn message_run_retry_is_limited_to_failed_nodes() {
        require_failed_message_run(&TaskRunStatus::Failed).expect("failed run should be retryable");
        for status in [
            TaskRunStatus::Queued,
            TaskRunStatus::Running,
            TaskRunStatus::Succeeded,
            TaskRunStatus::Blocked,
            TaskRunStatus::Cancelled,
        ] {
            let error = require_failed_message_run(&status)
                .expect_err("non-failed run must not be retried from a message task card");
            assert_eq!(error.status, StatusCode::BAD_REQUEST);
        }
    }
}
