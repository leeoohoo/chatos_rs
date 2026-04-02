use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{CreateTaskExecutionMessageRequest, TaskExecutionComposeRequest};
use crate::repositories::{task_execution_messages, task_execution_summaries};

use super::{ensure_agent_read_access, require_auth, resolve_scope_user_id, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct ListTaskExecutionMessagesQuery {
    user_id: Option<String>,
    contact_agent_id: String,
    project_id: String,
    limit: Option<i64>,
    offset: Option<i64>,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskExecutionScopeQuery {
    user_id: Option<String>,
    contact_agent_id: String,
    project_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct SyncTaskExecutionMessageRequest {
    user_id: Option<String>,
    contact_agent_id: String,
    project_id: String,
    task_id: Option<String>,
    source_session_id: Option<String>,
    role: String,
    content: String,
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
    created_at: Option<String>,
}

async fn ensure_scope_access(
    state: &SharedState,
    headers: &HeaderMap,
    user_id: Option<String>,
    contact_agent_id: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let auth = require_auth(state, headers)?;
    let scope_user_id = resolve_scope_user_id(&auth, user_id);
    if let Err(err) = ensure_agent_read_access(state.as_ref(), &auth, contact_agent_id).await {
        return Err(err);
    }
    Ok(scope_user_id)
}

fn require_internal_scope_user_id(
    user_id: Option<String>,
) -> Result<String, (StatusCode, Json<Value>)> {
    user_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "user_id is required"})),
            )
        })
}

pub(super) async fn create_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateTaskExecutionMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match ensure_scope_access(
        &state,
        &headers,
        Some(req.user_id.clone()),
        req.contact_agent_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return err,
    };

    let payload = CreateTaskExecutionMessageRequest {
        user_id: scope_user_id,
        ..req
    };
    match task_execution_messages::create_message(&state.pool, payload).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create task execution message failed", "detail": err})),
        ),
    }
}

pub(super) async fn sync_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Json(req): Json<SyncTaskExecutionMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match ensure_scope_access(
        &state,
        &headers,
        req.user_id.clone(),
        req.contact_agent_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return err,
    };

    let created_at = req
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let input = task_execution_messages::SyncTaskExecutionMessageInput {
        message_id,
        user_id: scope_user_id,
        contact_agent_id: req.contact_agent_id,
        project_id: req.project_id,
        task_id: req.task_id,
        source_session_id: req.source_session_id,
        role: req.role,
        content: req.content,
        message_mode: req.message_mode,
        message_source: req.message_source,
        tool_calls_json: req.tool_calls.map(|v| v.to_string()),
        tool_call_id: req.tool_call_id,
        reasoning: req.reasoning,
        metadata_json: req.metadata.map(|v| v.to_string()),
        created_at,
    };

    match task_execution_messages::upsert_message_sync(&state.pool, input).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync task execution message failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_sync_message(
    State(state): State<SharedState>,
    Path(message_id): Path<String>,
    Json(req): Json<SyncTaskExecutionMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match require_internal_scope_user_id(req.user_id.clone()) {
        Ok(value) => value,
        Err(err) => return err,
    };

    let created_at = req
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let input = task_execution_messages::SyncTaskExecutionMessageInput {
        message_id,
        user_id: scope_user_id,
        contact_agent_id: req.contact_agent_id,
        project_id: req.project_id,
        task_id: req.task_id,
        source_session_id: req.source_session_id,
        role: req.role,
        content: req.content,
        message_mode: req.message_mode,
        message_source: req.message_source,
        tool_calls_json: req.tool_calls.map(|v| v.to_string()),
        tool_call_id: req.tool_call_id,
        reasoning: req.reasoning,
        metadata_json: req.metadata.map(|v| v.to_string()),
        created_at,
    };

    match task_execution_messages::upsert_message_sync(&state.pool, input).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync task execution message failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListTaskExecutionMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match ensure_scope_access(
        &state,
        &headers,
        q.user_id.clone(),
        q.contact_agent_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return err,
    };

    let asc = !matches!(q.order.as_deref(), Some("desc"));
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);

    match task_execution_messages::list_messages(
        &state.pool,
        scope_user_id.as_str(),
        q.contact_agent_id.as_str(),
        q.project_id.as_str(),
        limit,
        offset,
        asc,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list task execution messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_list_messages(
    State(state): State<SharedState>,
    Query(q): Query<ListTaskExecutionMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match require_internal_scope_user_id(q.user_id.clone()) {
        Ok(value) => value,
        Err(err) => return err,
    };

    let asc = !matches!(q.order.as_deref(), Some("desc"));
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);

    match task_execution_messages::list_messages(
        &state.pool,
        scope_user_id.as_str(),
        q.contact_agent_id.as_str(),
        q.project_id.as_str(),
        limit,
        offset,
        asc,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list task execution messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn clear_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<TaskExecutionScopeQuery>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match ensure_scope_access(
        &state,
        &headers,
        q.user_id.clone(),
        q.contact_agent_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return err,
    };

    match task_execution_messages::delete_messages(
        &state.pool,
        scope_user_id.as_str(),
        q.contact_agent_id.as_str(),
        q.project_id.as_str(),
    )
    .await
    {
        Ok(deleted) => (
            StatusCode::OK,
            Json(json!({"success": true, "deleted": deleted})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "clear task execution messages failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_summaries(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListTaskExecutionMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match ensure_scope_access(
        &state,
        &headers,
        q.user_id.clone(),
        q.contact_agent_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return err,
    };

    match task_execution_summaries::list_summaries(
        &state.pool,
        scope_user_id.as_str(),
        q.contact_agent_id.as_str(),
        q.project_id.as_str(),
        None,
        None,
        q.limit.unwrap_or(100),
        q.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list task execution summaries failed", "detail": err})),
        ),
    }
}

pub(super) async fn delete_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(summary_id): Path<String>,
    Query(q): Query<TaskExecutionScopeQuery>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match ensure_scope_access(
        &state,
        &headers,
        q.user_id.clone(),
        q.contact_agent_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return err,
    };

    match task_execution_summaries::delete_summary(
        &state.pool,
        scope_user_id.as_str(),
        q.contact_agent_id.as_str(),
        q.project_id.as_str(),
        summary_id.as_str(),
    )
    .await
    {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task execution summary not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete task execution summary failed", "detail": err})),
        ),
    }
}

pub(super) async fn compose_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<TaskExecutionComposeRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match ensure_scope_access(
        &state,
        &headers,
        Some(req.user_id.clone()),
        req.contact_agent_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return err,
    };

    let payload = TaskExecutionComposeRequest {
        user_id: scope_user_id,
        ..req
    };

    match crate::services::context::compose_task_execution_context(&state.pool, payload).await {
        Ok(ctx) => (StatusCode::OK, Json(json!(ctx))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "compose task execution context failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_compose_context(
    State(state): State<SharedState>,
    Json(req): Json<TaskExecutionComposeRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match require_internal_scope_user_id(Some(req.user_id.clone())) {
        Ok(value) => value,
        Err(err) => return err,
    };

    let payload = TaskExecutionComposeRequest {
        user_id: scope_user_id,
        ..req
    };

    match crate::services::context::compose_task_execution_context(&state.pool, payload).await {
        Ok(ctx) => (StatusCode::OK, Json(json!(ctx))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "compose task execution context failed", "detail": err})),
        ),
    }
}
