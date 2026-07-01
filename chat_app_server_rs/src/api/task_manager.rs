// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::services::task_manager::{
    complete_task_by_id, delete_task_by_id, list_tasks_for_context, update_task_by_id,
    TaskOutcomeItem, TaskUpdatePatch, TASK_NOT_FOUND_ERR,
};

#[derive(Debug, Deserialize)]
struct TaskListQuery {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: String,
    conversation_turn_id: Option<String>,
    include_done: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SessionScopeQuery {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: String,
}

#[derive(Debug, Deserialize)]
struct UpdateTaskRequest {
    title: Option<String>,
    details: Option<String>,
    description: Option<String>,
    priority: Option<String>,
    status: Option<String>,
    tags: Option<Vec<String>>,
    due_at: Option<Option<String>>,
    #[serde(rename = "dueAt")]
    due_at_legacy: Option<Option<String>>,
    outcome_summary: Option<String>,
    outcome_items: Option<Vec<TaskOutcomeItem>>,
    resume_hint: Option<String>,
    blocker_reason: Option<String>,
    blocker_needs: Option<Vec<String>>,
    blocker_kind: Option<String>,
    completed_at: Option<Option<String>>,
    #[serde(rename = "completedAt")]
    completed_at_legacy: Option<Option<String>>,
    last_outcome_at: Option<Option<String>>,
    #[serde(rename = "lastOutcomeAt")]
    last_outcome_at_legacy: Option<Option<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct CompleteTaskRequest {
    outcome_summary: Option<String>,
    outcome_items: Option<Vec<TaskOutcomeItem>>,
    resume_hint: Option<String>,
    completed_at: Option<Option<String>>,
    #[serde(rename = "completedAt")]
    completed_at_legacy: Option<Option<String>>,
    last_outcome_at: Option<Option<String>>,
    #[serde(rename = "lastOutcomeAt")]
    last_outcome_at_legacy: Option<Option<String>>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/task-manager/tasks", get(list_tasks))
        .route(
            "/api/task-manager/tasks/:task_id",
            patch(update_task).delete(delete_task),
        )
        .route(
            "/api/task-manager/tasks/:task_id/complete",
            post(complete_task),
        )
}

async fn list_tasks(
    auth: AuthUser,
    Query(query): Query<TaskListQuery>,
) -> (StatusCode, Json<Value>) {
    if query.conversation_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "conversation_id is required" })),
        );
    }
    if let Err(err) = ensure_owned_session(query.conversation_id.as_str(), &auth).await {
        return map_session_access_error_with_success(err);
    }

    let include_done = query.include_done.unwrap_or(false);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    match list_tasks_for_context(
        query.conversation_id.as_str(),
        query.conversation_turn_id.as_deref(),
        include_done,
        limit,
    )
    .await
    {
        Ok(tasks) => {
            let payload = json!({
                "success": true,
                "conversation_id": query.conversation_id,
                "conversationId": query.conversation_id,
                "count": tasks.len(),
                "tasks": tasks,
            });
            (StatusCode::OK, Json(payload))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn update_task(
    auth: AuthUser,
    Path(task_id): Path<String>,
    Query(scope): Query<SessionScopeQuery>,
    Json(req): Json<UpdateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if scope.conversation_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "conversation_id is required" })),
        );
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "task_id is required" })),
        );
    }
    if let Err(err) = ensure_owned_session(scope.conversation_id.as_str(), &auth).await {
        return map_session_access_error_with_success(err);
    }

    let patch = TaskUpdatePatch {
        title: req.title,
        details: req.details.or(req.description),
        priority: req.priority,
        status: req.status,
        tags: req.tags,
        due_at: req.due_at.or(req.due_at_legacy),
        outcome_summary: req.outcome_summary,
        outcome_items: req.outcome_items,
        resume_hint: req.resume_hint,
        blocker_reason: req.blocker_reason,
        blocker_needs: req.blocker_needs,
        blocker_kind: req.blocker_kind,
        completed_at: req.completed_at.or(req.completed_at_legacy),
        last_outcome_at: req.last_outcome_at.or(req.last_outcome_at_legacy),
    };

    let empty_patch = patch.title.is_none()
        && patch.details.is_none()
        && patch.priority.is_none()
        && patch.status.is_none()
        && patch.tags.is_none()
        && patch.due_at.is_none()
        && patch.outcome_summary.is_none()
        && patch.outcome_items.is_none()
        && patch.resume_hint.is_none()
        && patch.blocker_reason.is_none()
        && patch.blocker_needs.is_none()
        && patch.blocker_kind.is_none()
        && patch.completed_at.is_none()
        && patch.last_outcome_at.is_none();
    if empty_patch {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "at least one field is required" })),
        );
    }

    match update_task_by_id(scope.conversation_id.as_str(), task_id.as_str(), patch).await {
        Ok(task) => {
            let payload = json!({
                "success": true,
                "conversation_id": scope.conversation_id,
                "conversationId": scope.conversation_id,
                "task": task,
            });
            (StatusCode::OK, Json(payload))
        }
        Err(err) if err == TASK_NOT_FOUND_ERR => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "error": err })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn complete_task(
    auth: AuthUser,
    Path(task_id): Path<String>,
    Query(scope): Query<SessionScopeQuery>,
    Json(req): Json<CompleteTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if scope.conversation_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "conversation_id is required" })),
        );
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "task_id is required" })),
        );
    }
    if let Err(err) = ensure_owned_session(scope.conversation_id.as_str(), &auth).await {
        return map_session_access_error_with_success(err);
    }

    let patch = TaskUpdatePatch {
        outcome_summary: req.outcome_summary,
        outcome_items: req.outcome_items,
        resume_hint: req.resume_hint,
        completed_at: req.completed_at.or(req.completed_at_legacy),
        last_outcome_at: req.last_outcome_at.or(req.last_outcome_at_legacy),
        ..TaskUpdatePatch::default()
    };

    match complete_task_by_id(
        scope.conversation_id.as_str(),
        task_id.as_str(),
        Some(patch),
    )
    .await
    {
        Ok(task) => {
            let payload = json!({
                "success": true,
                "conversation_id": scope.conversation_id,
                "conversationId": scope.conversation_id,
                "task": task,
            });
            (StatusCode::OK, Json(payload))
        }
        Err(err) if err == TASK_NOT_FOUND_ERR => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "error": err })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn delete_task(
    auth: AuthUser,
    Path(task_id): Path<String>,
    Query(scope): Query<SessionScopeQuery>,
) -> (StatusCode, Json<Value>) {
    if scope.conversation_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "conversation_id is required" })),
        );
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "task_id is required" })),
        );
    }
    if let Err(err) = ensure_owned_session(scope.conversation_id.as_str(), &auth).await {
        return map_session_access_error_with_success(err);
    }

    match delete_task_by_id(scope.conversation_id.as_str(), task_id.as_str()).await {
        Ok(true) => {
            let payload = json!({
                "success": true,
                "conversation_id": scope.conversation_id,
                "conversationId": scope.conversation_id,
                "deleted": true
            });
            (StatusCode::OK, Json(payload))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "deleted": false, "error": TASK_NOT_FOUND_ERR })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}
