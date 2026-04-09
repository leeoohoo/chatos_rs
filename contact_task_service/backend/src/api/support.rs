use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::{require_auth, AuthIdentity};
use crate::models::{ContactTask, TaskResultBriefView, UpdateTaskPlanRequest, UpdateTaskRequest};
use crate::repository;

use super::{ListTasksQuery, SharedState, MEMORY_HTTP};

pub(super) fn visible_user_ids(auth: &AuthIdentity, requested: Option<String>) -> Vec<String> {
    if auth.is_admin() {
        match requested
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            Some(user_id) => vec![user_id],
            None => Vec::new(),
        }
    } else {
        vec![auth.user_id.clone()]
    }
}

pub(super) fn internal_visible_user_ids(requested: Option<String>) -> Vec<String> {
    requested
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|user_id| vec![user_id])
        .unwrap_or_default()
}

pub(super) fn memory_timeout_duration(state: &SharedState) -> Duration {
    Duration::from_millis(state.config.memory_server_request_timeout_ms.max(300))
}

pub(super) fn build_memory_url(state: &SharedState, path: &str) -> String {
    format!(
        "{}{}",
        state.config.memory_server_base_url.trim_end_matches('/'),
        path
    )
}

pub(super) fn not_found_task_response() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({"error": "task not found"})))
}

pub(super) fn forbidden_response() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})))
}

pub(super) fn scope_mismatch_response() -> (StatusCode, Json<Value>) {
    (StatusCode::FORBIDDEN, Json(json!({"error": "scope mismatch"})))
}

pub(super) async fn load_task_or_http_error(
    state: &SharedState,
    task_id: &str,
) -> Result<ContactTask, (StatusCode, Json<Value>)> {
    match repository::get_task(&state.db, task_id).await {
        Ok(Some(task)) => Ok(task),
        Ok(None) => Err(not_found_task_response()),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load task failed", "detail": err})),
        )),
    }
}

fn normalize_requested_user_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn resolve_scope_user_id_ref(auth: &AuthIdentity, requested_user_id: Option<&str>) -> String {
    if auth.is_admin() {
        normalize_requested_user_id(requested_user_id).unwrap_or_else(|| auth.user_id.clone())
    } else {
        auth.user_id.clone()
    }
}

pub(super) fn ensure_public_task_scope(
    auth: &AuthIdentity,
    task_owner_user_id: &str,
    requested_user_id: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    if !auth.is_admin() && task_owner_user_id != auth.user_id {
        return Err(forbidden_response());
    }
    let scope_user_id = resolve_scope_user_id_ref(auth, requested_user_id);
    if task_owner_user_id != scope_user_id {
        return Err(scope_mismatch_response());
    }
    Ok(())
}

pub(super) fn ensure_internal_task_scope(
    task_owner_user_id: &str,
    requested_user_id: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    if let Some(scope_user_id) = normalize_requested_user_id(requested_user_id) {
        if task_owner_user_id != scope_user_id {
            return Err(scope_mismatch_response());
        }
    }
    Ok(())
}

pub(super) fn ensure_owner_or_admin(
    auth: &AuthIdentity,
    task_owner_user_id: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    if !auth.is_admin() && task_owner_user_id != auth.user_id {
        return Err(forbidden_response());
    }
    Ok(())
}

pub(super) fn require_scope_user_id(
    requested_user_id: Option<String>,
) -> Result<String, (StatusCode, Json<Value>)> {
    let scope_user_id = requested_user_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    scope_user_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "user_id is required"})),
        )
    })
}

pub(super) async fn ensure_public_task_action_scope(
    state: &SharedState,
    headers: &HeaderMap,
    task_id: &str,
    requested_user_id: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    let auth = require_auth(headers, &state.config).await?;
    let existing = load_task_or_http_error(state, task_id).await?;
    ensure_public_task_scope(&auth, existing.user_id.as_str(), requested_user_id)
}

pub(super) async fn ensure_internal_task_action_scope(
    state: &SharedState,
    task_id: &str,
    requested_user_id: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    let existing = load_task_or_http_error(state, task_id).await?;
    ensure_internal_task_scope(existing.user_id.as_str(), requested_user_id)
}

pub(super) fn map_task_action_result(
    action_name: &str,
    result: Result<Option<ContactTask>, String>,
) -> (StatusCode, Json<Value>) {
    match result {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => not_found_task_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{} failed", action_name), "detail": err})),
        ),
    }
}

pub(super) async fn list_tasks_with_scope(
    state: &SharedState,
    visible_user_ids: &[String],
    q: &ListTasksQuery,
) -> (StatusCode, Json<Value>) {
    match repository::list_tasks(
        &state.db,
        visible_user_ids,
        q.contact_agent_id.as_deref(),
        q.project_id.as_deref(),
        q.session_id.as_deref(),
        q.conversation_turn_id.as_deref(),
        q.status.as_deref(),
        q.limit.unwrap_or(100),
        q.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list tasks failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_task_plans_with_scope(
    state: &SharedState,
    visible_user_ids: &[String],
    q: &ListTasksQuery,
) -> (StatusCode, Json<Value>) {
    match repository::list_task_plans(
        &state.db,
        visible_user_ids,
        q.contact_agent_id.as_deref(),
        q.project_id.as_deref(),
        q.session_id.as_deref(),
        q.conversation_turn_id.as_deref(),
        q.status.as_deref(),
        q.limit.unwrap_or(100),
        q.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list task plans failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_task_plan_with_scope(
    state: &SharedState,
    plan_id: &str,
    visible_user_ids: &[String],
) -> (StatusCode, Json<Value>) {
    match repository::get_task_plan(&state.db, plan_id, visible_user_ids).await {
        Ok(Some(plan)) => (StatusCode::OK, Json(json!({ "item": plan }))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task plan not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get task plan failed", "detail": err})),
        ),
    }
}

pub(super) async fn scheduler_next_for_scope(
    state: &SharedState,
    scope_user_id: &str,
    req: &crate::models::SchedulerRequest,
) -> (StatusCode, Json<Value>) {
    match repository::scheduler_next(
        &state.db,
        scope_user_id,
        req.contact_agent_id.as_str(),
        req.project_id.as_str(),
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(json!(result))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "scheduler next failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_scheduler_scopes_with_user_ids(
    state: &SharedState,
    visible_user_ids: &[String],
    limit: i64,
) -> (StatusCode, Json<Value>) {
    match repository::list_scheduler_scopes(&state.db, visible_user_ids, limit).await {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list scheduler scopes failed", "detail": err})),
        ),
    }
}

pub(super) async fn ack_all_done_for_scope(
    state: &SharedState,
    scope_user_id: &str,
    req: &crate::models::AckAllDoneRequest,
) -> (StatusCode, Json<Value>) {
    let ack_at = req
        .ack_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    match repository::ack_all_done(
        &state.db,
        scope_user_id,
        req.contact_agent_id.as_str(),
        req.project_id.as_str(),
        ack_at.as_str(),
    )
    .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({"success": true, "ack_at": ack_at})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "ack all done failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_task_response(
    state: &SharedState,
    task_id: &str,
    req: UpdateTaskRequest,
) -> (StatusCode, Json<Value>) {
    match repository::update_task(&state.db, task_id, req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => not_found_task_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update task failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_task_plan_response(
    state: &SharedState,
    plan_id: &str,
    req: UpdateTaskPlanRequest,
) -> (StatusCode, Json<Value>) {
    match repository::update_task_plan(&state.db, plan_id, req).await {
        Ok(Some(resp)) => (
            StatusCode::OK,
            Json(json!({
                "item": resp.item,
                "operation_results": resp.operation_results,
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task plan not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update task plan failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_task_response(
    state: &SharedState,
    task_id: &str,
    auth: Option<&AuthIdentity>,
) -> (StatusCode, Json<Value>) {
    let task = match load_task_or_http_error(state, task_id).await {
        Ok(task) => task,
        Err(err) => return err,
    };
    if let Some(identity) = auth {
        if let Err(err) = ensure_owner_or_admin(identity, task.user_id.as_str()) {
            return err;
        }
    }
    (StatusCode::OK, Json(json!(task)))
}

pub(super) async fn get_task_result_brief_response_by_task(
    state: &SharedState,
    task: &ContactTask,
) -> (StatusCode, Json<Value>) {
    let resp = match MEMORY_HTTP
        .get(build_memory_url(
            state,
            format!("/internal/task-result-briefs/by-task/{}", task.id.as_str()).as_str(),
        ))
        .timeout(memory_timeout_duration(state))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "get task result brief failed", "detail": err.to_string()})),
            )
        }
    };
    if resp.status().as_u16() == 404 {
        return (StatusCode::OK, Json(json!({ "item": Value::Null })));
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return (
            StatusCode::BAD_GATEWAY,
            Json(
                json!({"error": "get task result brief failed", "detail": format!("status={} body={}", status, detail)}),
            ),
        );
    }

    let item = match resp.json::<TaskResultBriefView>().await {
        Ok(item) => item,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({"error": "invalid task result brief response", "detail": err.to_string()}),
                ),
            )
        }
    };

    (StatusCode::OK, Json(json!({ "item": item })))
}
