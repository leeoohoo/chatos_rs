use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::services::task_manager::{
    complete_task_by_id, delete_task_by_id, list_tasks_for_context, submit_task_review_decision,
    update_task_by_id, TaskDraft, TaskReviewAction, TaskUpdatePatch, REVIEW_NOT_FOUND_ERR,
    TASK_NOT_FOUND_ERR,
};

#[derive(Debug, Deserialize)]
struct ReviewDecisionRequest {
    action: TaskReviewAction,
    tasks: Option<Vec<TaskDraft>>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskListQuery {
    session_id: String,
    conversation_turn_id: Option<String>,
    include_done: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SessionScopeQuery {
    session_id: String,
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
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/task-manager/reviews/:review_id/decision",
            post(submit_review_decision),
        )
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

async fn list_tasks(Query(query): Query<TaskListQuery>) -> (StatusCode, Json<Value>) {
    if query.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "session_id is required" })),
        );
    }

    let include_done = query.include_done.unwrap_or(false);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    match list_tasks_for_context(
        query.session_id.as_str(),
        query.conversation_turn_id.as_deref(),
        include_done,
        limit,
    )
    .await
    {
        Ok(tasks) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "count": tasks.len(),
                "tasks": tasks,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn update_task(
    Path(task_id): Path<String>,
    Query(scope): Query<SessionScopeQuery>,
    Json(req): Json<UpdateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if scope.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "session_id is required" })),
        );
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "task_id is required" })),
        );
    }

    let patch = TaskUpdatePatch {
        title: req.title,
        details: req.details.or(req.description),
        priority: req.priority,
        status: req.status,
        tags: req.tags,
        due_at: req.due_at.or(req.due_at_legacy),
    };

    let empty_patch = patch.title.is_none()
        && patch.details.is_none()
        && patch.priority.is_none()
        && patch.status.is_none()
        && patch.tags.is_none()
        && patch.due_at.is_none();
    if empty_patch {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "at least one field is required" })),
        );
    }

    match update_task_by_id(scope.session_id.as_str(), task_id.as_str(), patch).await {
        Ok(task) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "task": task,
            })),
        ),
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
    Path(task_id): Path<String>,
    Query(scope): Query<SessionScopeQuery>,
) -> (StatusCode, Json<Value>) {
    if scope.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "session_id is required" })),
        );
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "task_id is required" })),
        );
    }

    match complete_task_by_id(scope.session_id.as_str(), task_id.as_str()).await {
        Ok(task) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "task": task,
            })),
        ),
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
    Path(task_id): Path<String>,
    Query(scope): Query<SessionScopeQuery>,
) -> (StatusCode, Json<Value>) {
    if scope.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "session_id is required" })),
        );
    }
    if task_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "task_id is required" })),
        );
    }

    match delete_task_by_id(scope.session_id.as_str(), task_id.as_str()).await {
        Ok(true) => (
            StatusCode::OK,
            Json(json!({ "success": true, "deleted": true })),
        ),
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

async fn submit_review_decision(
    Path(review_id): Path<String>,
    Json(req): Json<ReviewDecisionRequest>,
) -> (StatusCode, Json<Value>) {
    if review_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "review_id is required" })),
        );
    }

    if matches!(req.action, TaskReviewAction::Confirm) {
        let empty = req
            .tasks
            .as_ref()
            .map(|tasks| tasks.is_empty())
            .unwrap_or(true);
        if empty {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "tasks is required for confirm action" })),
            );
        }
    }

    match submit_task_review_decision(review_id.as_str(), req.action, req.tasks, req.reason).await {
        Ok(payload) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "review_id": payload.review_id,
                "session_id": payload.session_id,
                "conversation_turn_id": payload.conversation_turn_id,
                "action": req.action.as_str(),
            })),
        ),
        Err(err) if err == REVIEW_NOT_FOUND_ERR => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "error": err })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}
