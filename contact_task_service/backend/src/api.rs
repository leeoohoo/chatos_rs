use std::sync::Arc;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{login_via_memory, require_auth, resolve_scope_user_id};
use crate::models::{
    AckAllDoneRequest, AckPauseTaskRequest, AckStopTaskRequest, ConfirmTaskRequest,
    CreateTaskRequest, LoginRequest, PauseTaskRequest, ResumeTaskRequest, RetryTaskRequest,
    SchedulerRequest, StopTaskRequest, TaskExecutionMessageView, UpdateTaskPlanRequest,
    UpdateTaskRequest,
};
use crate::{repository, AppState};

type SharedState = Arc<AppState>;
static MEMORY_HTTP: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
mod support;
use self::support::*;

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
    let user_ids = visible_user_ids(&auth, q.user_id.clone());
    list_tasks_with_scope(&state, user_ids.as_slice(), &q).await
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
    let user_ids = visible_user_ids(&auth, q.user_id.clone());
    list_task_plans_with_scope(&state, user_ids.as_slice(), &q).await
}

async fn internal_create_task(
    State(state): State<SharedState>,
    Json(req): Json<CreateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match require_scope_user_id(req.user_id.clone()) {
        Ok(user_id) => user_id,
        Err(err) => return err,
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
    let user_ids = internal_visible_user_ids(q.user_id.clone());
    list_task_plans_with_scope(&state, user_ids.as_slice(), &q).await
}

async fn internal_list_tasks(
    State(state): State<SharedState>,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    let user_ids = internal_visible_user_ids(q.user_id.clone());
    list_tasks_with_scope(&state, user_ids.as_slice(), &q).await
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
    let user_ids = visible_user_ids(&auth, None);
    get_task_plan_with_scope(&state, plan_id.as_str(), user_ids.as_slice()).await
}

async fn internal_get_task_plan(
    State(state): State<SharedState>,
    Path(plan_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    get_task_plan_with_scope(&state, plan_id.as_str(), &[]).await
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
    get_task_response(&state, task_id.as_str(), Some(&auth)).await
}

async fn internal_get_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    get_task_response(&state, task_id.as_str(), None).await
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
    let task = match load_task_or_http_error(&state, task_id.as_str()).await {
        Ok(task) => task,
        Err(err) => return err,
    };
    if let Err(err) = ensure_owner_or_admin(&auth, task.user_id.as_str()) {
        return err;
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
    let task = match load_task_or_http_error(&state, task_id.as_str()).await {
        Ok(task) => task,
        Err(err) => return err,
    };
    if let Err(err) = ensure_owner_or_admin(&auth, task.user_id.as_str()) {
        return err;
    }

    get_task_result_brief_response_by_task(&state, &task).await
}

async fn internal_get_task_result_brief(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let task = match load_task_or_http_error(&state, task_id.as_str()).await {
        Ok(task) => task,
        Err(err) => return err,
    };

    get_task_result_brief_response_by_task(&state, &task).await
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
    let existing = match load_task_or_http_error(&state, task_id.as_str()).await {
        Ok(task) => task,
        Err(err) => return err,
    };
    if let Err(err) = ensure_owner_or_admin(&auth, existing.user_id.as_str()) {
        return err;
    }
    update_task_response(&state, task_id.as_str(), req).await
}

async fn internal_update_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> (StatusCode, Json<Value>) {
    update_task_response(&state, task_id.as_str(), req).await
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
    if let Err(err) = ensure_owner_or_admin(&auth, existing.user_id.as_str()) {
        return err;
    }
    update_task_plan_response(&state, plan_id.as_str(), req).await
}

async fn internal_update_task_plan(
    State(state): State<SharedState>,
    Path(plan_id): Path<String>,
    Json(req): Json<UpdateTaskPlanRequest>,
) -> (StatusCode, Json<Value>) {
    update_task_plan_response(&state, plan_id.as_str(), req).await
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
    let existing = match load_task_or_http_error(&state, task_id.as_str()).await {
        Ok(task) => task,
        Err(err) => return err,
    };
    if let Err(err) = ensure_owner_or_admin(&auth, existing.user_id.as_str()) {
        return err;
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
    if let Err(err) =
        ensure_public_task_action_scope(&state, &headers, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "confirm task",
        repository::confirm_task(&state.db, task_id.as_str(), req.note).await,
    )
}

async fn internal_confirm_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<ConfirmTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_internal_task_action_scope(&state, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "confirm task",
        repository::confirm_task(&state.db, task_id.as_str(), req.note).await,
    )
}

async fn request_pause_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<PauseTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_public_task_action_scope(&state, &headers, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "request pause task",
        repository::request_pause_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn internal_request_pause_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<PauseTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_internal_task_action_scope(&state, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "request pause task",
        repository::request_pause_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn request_stop_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<StopTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_public_task_action_scope(&state, &headers, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "request stop task",
        repository::request_stop_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn internal_request_stop_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<StopTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_internal_task_action_scope(&state, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "request stop task",
        repository::request_stop_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn resume_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<ResumeTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_public_task_action_scope(&state, &headers, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "resume task",
        repository::resume_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn internal_resume_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<ResumeTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_internal_task_action_scope(&state, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "resume task",
        repository::resume_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn retry_task(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(req): Json<RetryTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_public_task_action_scope(&state, &headers, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "retry task",
        repository::retry_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn internal_retry_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<RetryTaskRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) =
        ensure_internal_task_action_scope(&state, task_id.as_str(), req.user_id.as_deref()).await
    {
        return err;
    }
    map_task_action_result(
        "retry task",
        repository::retry_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn internal_ack_pause_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<AckPauseTaskRequest>,
) -> (StatusCode, Json<Value>) {
    map_task_action_result(
        "ack pause task",
        repository::ack_pause_task(&state.db, task_id.as_str(), req).await,
    )
}

async fn internal_ack_stop_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    Json(req): Json<AckStopTaskRequest>,
) -> (StatusCode, Json<Value>) {
    map_task_action_result(
        "ack stop task",
        repository::ack_stop_task(&state.db, task_id.as_str(), req).await,
    )
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
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    scheduler_next_for_scope(&state, scope_user_id.as_str(), &req).await
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
    let user_ids = visible_user_ids(&auth, q.user_id);
    list_scheduler_scopes_with_user_ids(&state, user_ids.as_slice(), q.limit.unwrap_or(500)).await
}

async fn internal_scheduler_next(
    State(state): State<SharedState>,
    Json(req): Json<SchedulerRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match require_scope_user_id(req.user_id.clone()) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    scheduler_next_for_scope(&state, scope_user_id.as_str(), &req).await
}

async fn internal_list_scheduler_scopes(
    State(state): State<SharedState>,
    Query(q): Query<ListTasksQuery>,
) -> (StatusCode, Json<Value>) {
    let user_ids = internal_visible_user_ids(q.user_id);
    list_scheduler_scopes_with_user_ids(&state, user_ids.as_slice(), q.limit.unwrap_or(500)).await
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
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id.clone());
    ack_all_done_for_scope(&state, scope_user_id.as_str(), &req).await
}

async fn internal_ack_all_done(
    State(state): State<SharedState>,
    Json(req): Json<AckAllDoneRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match require_scope_user_id(req.user_id.clone()) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    ack_all_done_for_scope(&state, scope_user_id.as_str(), &req).await
}
