use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{login_via_memory, require_auth, resolve_scope_user_id, AuthIdentity};
use crate::models::{
    AckAllDoneRequest, AckPauseTaskRequest, AckStopTaskRequest, ConfirmTaskRequest,
    CreateTaskRequest, LoginRequest, PauseTaskRequest, ResumeTaskRequest, RetryTaskRequest,
    SchedulerRequest, StopTaskRequest, TaskExecutionMessageView, TaskResultBriefView,
    UpdateTaskPlanRequest, UpdateTaskRequest,
};
use crate::{repository, AppState};

type SharedState = Arc<AppState>;
static MEMORY_HTTP: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

#[derive(Debug, Deserialize)]
struct ListTasksQuery {
    user_id: Option<String>,
    contact_agent_id: Option<String>,
    project_id: Option<String>,
    session_id: Option<String>,
    conversation_turn_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/task-service/v1/auth/login", post(login))
        .route("/api/task-service/v1/auth/me", get(me))
        .route(
            "/api/task-service/v1/tasks",
            post(create_task).get(list_tasks),
        )
        .route("/api/task-service/v1/task-plans", get(list_task_plans))
        .route(
            "/api/task-service/v1/task-plans/:plan_id",
            get(get_task_plan).patch(update_task_plan),
        )
        .route(
            "/api/task-service/v1/internal/tasks",
            post(internal_create_task).get(internal_list_tasks),
        )
        .route(
            "/api/task-service/v1/internal/task-plans",
            get(internal_list_task_plans),
        )
        .route(
            "/api/task-service/v1/internal/task-plans/:plan_id",
            get(internal_get_task_plan).patch(internal_update_task_plan),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id",
            get(get_task).patch(update_task).delete(delete_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id",
            get(internal_get_task).patch(internal_update_task),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id/execution-messages",
            get(list_task_execution_messages),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id/result-brief",
            get(get_task_result_brief),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/result-brief",
            get(internal_get_task_result_brief),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id/confirm",
            post(confirm_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/confirm",
            post(internal_confirm_task),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id/request-pause",
            post(request_pause_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/request-pause",
            post(internal_request_pause_task),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id/request-stop",
            post(request_stop_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/request-stop",
            post(internal_request_stop_task),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id/resume",
            post(resume_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/resume",
            post(internal_resume_task),
        )
        .route(
            "/api/task-service/v1/tasks/:task_id/retry",
            post(retry_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/retry",
            post(internal_retry_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/ack-pause",
            post(internal_ack_pause_task),
        )
        .route(
            "/api/task-service/v1/internal/tasks/:task_id/ack-stop",
            post(internal_ack_stop_task),
        )
        .route("/api/task-service/v1/scheduler/next", post(scheduler_next))
        .route(
            "/api/task-service/v1/scheduler/scopes",
            get(list_scheduler_scopes),
        )
        .route(
            "/api/task-service/v1/internal/scheduler/next",
            post(internal_scheduler_next),
        )
        .route(
            "/api/task-service/v1/internal/scheduler/scopes",
            get(internal_list_scheduler_scopes),
        )
        .route(
            "/api/task-service/v1/scheduler/all-done/ack",
            post(ack_all_done),
        )
        .route(
            "/api/task-service/v1/internal/scheduler/all-done/ack",
            post(internal_ack_all_done),
        )
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "contact_task_service"}))
}

async fn login(
    State(state): State<SharedState>,
    Json(req): Json<LoginRequest>,
) -> (StatusCode, Json<Value>) {
    match login_via_memory(&state.config, &req).await {
        Ok(resp) => resp,
        Err(err) => err,
    }
}

async fn me(State(state): State<SharedState>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    match require_auth(&headers, &state.config).await {
        Ok(auth) => (
            StatusCode::OK,
            Json(json!({"username": auth.user_id, "role": auth.role})),
        ),
        Err(err) => err,
    }
}

fn visible_user_ids(auth: &AuthIdentity, requested: Option<String>) -> Vec<String> {
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

fn internal_visible_user_ids(requested: Option<String>) -> Vec<String> {
    requested
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|user_id| vec![user_id])
        .unwrap_or_default()
}

fn memory_timeout_duration(state: &SharedState) -> Duration {
    Duration::from_millis(state.config.memory_server_request_timeout_ms.max(300))
}

fn build_memory_url(state: &SharedState, path: &str) -> String {
    format!(
        "{}{}",
        state.config.memory_server_base_url.trim_end_matches('/'),
        path
    )
}

async fn create_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    match repository::create_task(
        &state.db,
        scope_user_id.as_str(),
        auth.user_id.as_str(),
        req,
    )
    .await
    {
        Ok(task) => (StatusCode::OK, Json(json!(task))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create task failed", "detail": err})),
        ),
    }
}

async fn list_tasks(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    match repository::list_tasks(
        &state.db,
        visible_user_ids(&auth, q.user_id).as_slice(),
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

async fn list_task_plans(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    match repository::list_task_plans(
        &state.db,
        visible_user_ids(&auth, q.user_id).as_slice(),
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

async fn internal_create_task(
    State(state): State<SharedState>,
    Json(req): Json<CreateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = req
        .user_id
        .clone()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(scope_user_id) = scope_user_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "user_id is required"})),
        );
    };
    match repository::create_task(
        &state.db,
        scope_user_id.as_str(),
        scope_user_id.as_str(),
        req,
    )
    .await
    {
        Ok(task) => (StatusCode::OK, Json(json!(task))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create task failed", "detail": err})),
        ),
    }
}

async fn internal_list_task_plans(
    State(state): State<SharedState>,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    match repository::list_task_plans(
        &state.db,
        internal_visible_user_ids(q.user_id).as_slice(),
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

async fn internal_list_tasks(
    State(state): State<SharedState>,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    match repository::list_tasks(
        &state.db,
        internal_visible_user_ids(q.user_id).as_slice(),
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

async fn get_task_plan(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(plan_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    match repository::get_task_plan(
        &state.db,
        plan_id.as_str(),
        visible_user_ids(&auth, None).as_slice(),
    )
    .await
    {
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

async fn internal_get_task_plan(
    State(state): State<SharedState>,
    Path(plan_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match repository::get_task_plan(&state.db, plan_id.as_str(), &[]).await {
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

async fn get_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) if auth.is_admin() || task.user_id == auth.user_id => {
            (StatusCode::OK, Json(json!(task)))
        }
        Ok(Some(_)) => (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get task failed", "detail": err})),
        ),
    }
}

async fn internal_get_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get task failed", "detail": err})),
        ),
    }
}

async fn list_task_execution_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let task = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && task.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let req = MEMORY_HTTP
        .get(build_memory_url(
            &state,
            "/internal/task-executions/messages",
        ))
        .timeout(memory_timeout_duration(&state))
        .query(&[
            ("user_id", task.user_id.as_str()),
            ("contact_agent_id", task.contact_agent_id.as_str()),
            ("project_id", task.project_id.as_str()),
            ("limit", "2000"),
            ("offset", "0"),
            ("order", "asc"),
        ]);

    let resp = match req.send().await {
        Ok(resp) => resp,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "list execution messages failed", "detail": err.to_string()})),
            )
        }
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return (
            StatusCode::BAD_GATEWAY,
            Json(
                json!({"error": "list execution messages failed", "detail": format!("status={} body={}", status, detail)}),
            ),
        );
    }

    let payload: Value = match resp.json().await {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({"error": "invalid execution messages response", "detail": err.to_string()}),
                ),
            )
        }
    };
    let items = payload
        .get("items")
        .and_then(|v| serde_json::from_value::<Vec<TaskExecutionMessageView>>(v.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|item| task_execution_message_matches_task(item, &task.id))
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(json!({ "items": items })))
}

async fn get_task_result_brief(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let task = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && task.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let resp = match MEMORY_HTTP
        .get(build_memory_url(
            &state,
            format!("/internal/task-result-briefs/by-task/{}", task.id.as_str()).as_str(),
        ))
        .timeout(memory_timeout_duration(&state))
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

async fn internal_get_task_result_brief(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let task = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };

    let resp = match MEMORY_HTTP
        .get(build_memory_url(
            &state,
            format!("/internal/task-result-briefs/by-task/{}", task.id.as_str()).as_str(),
        ))
        .timeout(memory_timeout_duration(&state))
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

fn task_execution_message_matches_task(item: &TaskExecutionMessageView, task_id: &str) -> bool {
    if item.task_id.as_deref() == Some(task_id) {
        return true;
    }

    item.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("task_execution"))
        .and_then(|value| value.get("task_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(task_id)
}

async fn update_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match repository::update_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update task failed", "detail": err})),
        ),
    }
}

async fn internal_update_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    match repository::update_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update task failed", "detail": err})),
        ),
    }
}

async fn update_task_plan(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(plan_id): Path<String>,
    Json(req): Json<UpdateTaskPlanRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task_plan(
        &state.db,
        plan_id.as_str(),
        visible_user_ids(&auth, None).as_slice(),
    )
    .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task plan not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task plan failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match repository::update_task_plan(&state.db, plan_id.as_str(), req).await {
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

async fn internal_update_task_plan(
    State(state): State<SharedState>,
    Path(plan_id): Path<String>,
    Json(req): Json<UpdateTaskPlanRequest>,
) -> (StatusCode, Json<Value>) {
    match repository::update_task_plan(&state.db, plan_id.as_str(), req).await {
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

async fn delete_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match repository::delete_task(&state.db, task_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete task failed", "detail": err})),
        ),
    }
}

async fn confirm_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<ConfirmTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    if existing.user_id != scope_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "scope mismatch"})),
        );
    }
    match repository::confirm_task(&state.db, task_id.as_str(), req.note).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "confirm task failed", "detail": err})),
        ),
    }
}

async fn internal_confirm_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<ConfirmTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if let Some(scope_user_id) = req
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if existing.user_id != scope_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "scope mismatch"})),
            );
        }
    }

    match repository::confirm_task(&state.db, task_id.as_str(), req.note).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "confirm task failed", "detail": err})),
        ),
    }
}

async fn request_pause_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<PauseTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    if existing.user_id != scope_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "scope mismatch"})),
        );
    }
    match repository::request_pause_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "request pause task failed", "detail": err})),
        ),
    }
}

async fn internal_request_pause_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<PauseTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if let Some(scope_user_id) = req
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if existing.user_id != scope_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "scope mismatch"})),
            );
        }
    }
    match repository::request_pause_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "request pause task failed", "detail": err})),
        ),
    }
}

async fn request_stop_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<StopTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    if existing.user_id != scope_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "scope mismatch"})),
        );
    }
    match repository::request_stop_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "request stop task failed", "detail": err})),
        ),
    }
}

async fn internal_request_stop_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<StopTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if let Some(scope_user_id) = req
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if existing.user_id != scope_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "scope mismatch"})),
            );
        }
    }
    match repository::request_stop_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "request stop task failed", "detail": err})),
        ),
    }
}

async fn resume_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<ResumeTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    if existing.user_id != scope_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "scope mismatch"})),
        );
    }
    match repository::resume_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "resume task failed", "detail": err})),
        ),
    }
}

async fn internal_resume_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<ResumeTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if let Some(scope_user_id) = req
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if existing.user_id != scope_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "scope mismatch"})),
            );
        }
    }
    match repository::resume_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "resume task failed", "detail": err})),
        ),
    }
}

async fn retry_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<RetryTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    if existing.user_id != scope_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "scope mismatch"})),
        );
    }
    match repository::retry_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "retry task failed", "detail": err})),
        ),
    }
}

async fn internal_retry_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<RetryTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match repository::get_task(&state.db, task_id.as_str()).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load task failed", "detail": err})),
            )
        }
    };
    if let Some(scope_user_id) = req
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if existing.user_id != scope_user_id {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "scope mismatch"})),
            );
        }
    }
    match repository::retry_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "retry task failed", "detail": err})),
        ),
    }
}

async fn internal_ack_pause_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<AckPauseTaskRequest>,
) -> (StatusCode, Json<Value>) {
    match repository::ack_pause_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "ack pause task failed", "detail": err})),
        ),
    }
}

async fn internal_ack_stop_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<AckStopTaskRequest>,
) -> (StatusCode, Json<Value>) {
    match repository::ack_stop_task(&state.db, task_id.as_str(), req).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!(task))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "ack stop task failed", "detail": err})),
        ),
    }
}

async fn scheduler_next(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<SchedulerRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    match repository::scheduler_next(
        &state.db,
        scope_user_id.as_str(),
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

async fn list_scheduler_scopes(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    match repository::list_scheduler_scopes(
        &state.db,
        visible_user_ids(&auth, q.user_id).as_slice(),
        q.limit.unwrap_or(500),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list scheduler scopes failed", "detail": err})),
        ),
    }
}

async fn internal_scheduler_next(
    State(state): State<SharedState>,
    Json(req): Json<SchedulerRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = req
        .user_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(scope_user_id) = scope_user_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "user_id is required"})),
        );
    };
    match repository::scheduler_next(
        &state.db,
        scope_user_id.as_str(),
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

async fn internal_list_scheduler_scopes(
    State(state): State<SharedState>,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    match repository::list_scheduler_scopes(
        &state.db,
        internal_visible_user_ids(q.user_id).as_slice(),
        q.limit.unwrap_or(500),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list scheduler scopes failed", "detail": err})),
        ),
    }
}

async fn ack_all_done(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<AckAllDoneRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&headers, &state.config).await {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let ack_at = req
        .ack_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    match repository::ack_all_done(
        &state.db,
        scope_user_id.as_str(),
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

async fn internal_ack_all_done(
    State(state): State<SharedState>,
    Json(req): Json<AckAllDoneRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = req
        .user_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(scope_user_id) = scope_user_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "user_id is required"})),
        );
    };
    let ack_at = req
        .ack_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    match repository::ack_all_done(
        &state.db,
        scope_user_id.as_str(),
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
