use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::UpsertTaskResultBriefRequest;
use crate::repositories::task_result_briefs;

use super::{ensure_agent_read_access, require_auth, resolve_scope_user_id, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct TaskResultBriefQuery {
    user_id: Option<String>,
    contact_agent_id: String,
    project_id: String,
    limit: Option<i64>,
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

pub(super) async fn list_task_result_briefs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<TaskResultBriefQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    if let Err(err) =
        ensure_agent_read_access(state.as_ref(), &auth, q.contact_agent_id.as_str()).await
    {
        return err;
    }

    match task_result_briefs::list_task_result_briefs(
        &state.pool,
        user_id.as_str(),
        q.contact_agent_id.as_str(),
        q.project_id.as_str(),
        q.limit.unwrap_or(20),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list task result briefs failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_upsert_task_result_brief(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(mut req): Json<UpsertTaskResultBriefRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match require_internal_scope_user_id(Some(req.user_id.clone())) {
        Ok(value) => value,
        Err(err) => return err,
    };

    req.task_id = task_id;
    req.user_id = user_id;

    match task_result_briefs::upsert_task_result_brief(&state.pool, req).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync task result brief failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_get_task_result_brief_by_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match task_result_briefs::get_task_result_brief_by_task_id(&state.pool, task_id.as_str()).await
    {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task result brief not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get task result brief failed", "detail": err})),
        ),
    }
}
