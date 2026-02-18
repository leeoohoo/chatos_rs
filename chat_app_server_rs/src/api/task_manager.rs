use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::services::task_manager::{
    list_tasks_for_context, submit_task_review_decision, TaskDraft, TaskReviewAction,
    REVIEW_NOT_FOUND_ERR,
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

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/task-manager/reviews/:review_id/decision",
            post(submit_review_decision),
        )
        .route("/api/task-manager/tasks", get(list_tasks))
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
