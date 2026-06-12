use std::collections::{HashSet, VecDeque};

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Extension, Json, Router};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth::CurrentUser;
use crate::mcp_server::{JsonRpcRequest, JsonRpcResponse, McpRequestContext};
use crate::models::{
    AgentTokenRequest, AgentTokenResponse, BatchTaskDeleteRequest, BatchTaskOperationResponse,
    BatchTaskRunRequest, BatchTaskStatusUpdateRequest, CancelUiPromptRequest,
    CreateModelConfigRequest, CreateRemoteServerRequest, CreateTaskRequest, CreateUserRequest,
    CurrentUserResponse, HealthResponse, LoginRequest, LoginResponse, McpCatalogEntry,
    McpPromptPreviewRequest, McpPromptPreviewResponse, McpServerInfo, ModelCatalogResponse,
    ModelConfigRecord, ModelConfigTestResponse, ModelConfigUsageRecord, PaginatedResponse,
    PreviewModelCatalogRequest, PromptListFilters, RecordTaskProcessRequest, RemoteServerRecord,
    RemoteServerTestResponse, RunListFilters, RunSummaryRecord, SetTaskPrerequisitesRequest,
    StartTaskRunRequest, SubmitUiPromptRequest, SystemConfigResponse, TaskDependencyGraph,
    TaskIndexResponse, TaskListFilters, TaskMemoryContextOptions, TaskMemoryContextResponse,
    TaskMemoryRecordsOptions, TaskMemoryRecordsResponse, TaskMemorySummaryResponse, TaskRecord,
    TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskStatsResponse, TaskStatus,
    TaskSummaryRecord, TestModelConfigRequest, TestRemoteServerRequest, UiPromptRecord,
    UiPromptStatus, UiPromptTaskCountRecord, UpdateModelConfigRequest, UpdateRemoteServerRequest,
    UpdateRuntimeSettingsRequest, UpdateTaskMcpRequest, UpdateTaskRequest, UpdateUserRequest,
    UserSummaryRecord,
};
use crate::services::{health, system_config};
use crate::state::AppState;

mod chatos_internal;

const RUN_EVENT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(750);
const TASK_RUNNER_SKILL_ZH_CN: &str = include_str!("../../../TASK_RUNNER_AI_SKILL.zh-CN.md");
const TASK_RUNNER_SKILL_EN_US: &str = include_str!("../../../TASK_RUNNER_AI_SKILL.en-US.md");

pub fn build_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/system/config", patch(update_system_config_handler))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/:id", patch(update_user).delete(delete_user))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/summaries", get(list_task_summaries))
        .route("/api/tasks/page", get(list_tasks_page))
        .route("/api/tasks/index", get(get_task_index))
        .route("/api/tasks/stats", get(get_task_stats))
        .route("/api/tasks/batch/status", post(batch_update_task_status))
        .route("/api/tasks/batch/delete", post(batch_delete_tasks))
        .route("/api/tasks/batch/runs", post(batch_start_task_runs))
        .route(
            "/api/tasks/:id",
            get(get_task).patch(update_task).delete(delete_task),
        )
        .route(
            "/api/tasks/:id/runs",
            get(list_task_runs).post(start_task_run),
        )
        .route("/api/tasks/:id/mcp", patch(update_task_mcp))
        .route(
            "/api/tasks/:id/prerequisites",
            get(list_task_prerequisites).put(set_task_prerequisites),
        )
        .route(
            "/api/tasks/:id/dependency-graph",
            get(get_task_dependency_graph),
        )
        .route("/api/tasks/:id/process-log", patch(record_task_process))
        .route(
            "/api/tasks/:id/mcp/prompt-preview",
            get(preview_task_mcp_prompt),
        )
        .route(
            "/api/tasks/:id/memory/context",
            get(get_task_memory_context),
        )
        .route(
            "/api/tasks/:id/memory/records",
            get(get_task_memory_records),
        )
        .route(
            "/api/tasks/:id/memory/summarize",
            post(summarize_task_memory),
        )
        .route(
            "/api/model-configs",
            get(list_model_configs).post(create_model_config),
        )
        .route(
            "/api/model-configs/catalog/preview",
            post(preview_model_catalog),
        )
        .route(
            "/api/model-configs/:id",
            get(get_model_config)
                .patch(update_model_config)
                .delete(delete_model_config),
        )
        .route("/api/model-configs/:id/models", get(list_model_catalog))
        .route("/api/model-configs/:id/test", post(test_model_config))
        .route("/api/model-configs/usage", get(list_model_config_usage))
        .route(
            "/api/remote-servers",
            get(list_remote_servers).post(create_remote_server),
        )
        .route("/api/remote-servers/test", post(test_remote_server_draft))
        .route(
            "/api/remote-servers/:id",
            get(get_remote_server)
                .patch(update_remote_server)
                .delete(delete_remote_server),
        )
        .route(
            "/api/remote-servers/:id/test",
            post(test_remote_server_saved),
        )
        .route("/api/runs", get(list_runs))
        .route("/api/runs/summaries", get(list_run_summaries))
        .route("/api/runs/page", get(list_runs_page))
        .route("/api/runs/index", get(list_run_index))
        .route("/api/runs/:id", get(get_run))
        .route("/api/runs/:id/events", get(list_run_events))
        .route("/api/runs/:id/prompts", get(list_run_prompts))
        .route("/api/runs/:id/stream", get(stream_run_events))
        .route("/api/runs/:id/cancel", post(cancel_run))
        .route("/api/runs/:id/retry", post(retry_run))
        .route("/api/prompts", get(list_prompts))
        .route("/api/prompts/page", get(list_prompts_page))
        .route("/api/prompts/task-counts", get(list_prompt_task_counts))
        .route("/api/prompts/:id", get(get_prompt))
        .route("/api/prompts/:id/submit", post(submit_prompt))
        .route("/api/prompts/:id/cancel", post(cancel_prompt))
        .route("/api/tooling/notepad/folders", get(list_notepad_folders))
        .route("/api/tooling/notepad/tags", get(list_notepad_tags))
        .route("/api/tooling/notepad/notes", get(list_notepad_notes))
        .route("/api/tooling/notepad/notes/:id", get(read_notepad_note))
        .route(
            "/api/tooling/terminal/processes",
            get(list_terminal_processes),
        )
        .route(
            "/api/tooling/terminal/processes/:id/logs",
            get(get_terminal_process_logs),
        )
        .route(
            "/api/tooling/terminal/processes/:id/kill",
            post(kill_terminal_process),
        )
        .route(
            "/api/tooling/terminal/processes/:id/write",
            post(write_terminal_process),
        )
        .route("/api/mcp/server", get(get_mcp_server_info))
        .route("/api/mcp/tools", get(list_mcp_catalog))
        .route("/api/mcp/prompt-preview", post(preview_mcp_prompt))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/system/config", get(system_config_handler))
        .route("/api/skills/task-runner", get(task_runner_skill_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/agent-token", post(agent_token_handler))
        .merge(chatos_internal::router())
        .merge(protected_api)
        .route("/mcp", post(mcp_entrypoint))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}

async fn health_handler() -> Json<HealthResponse> {
    Json(health())
}

async fn system_config_handler(
    State(state): State<AppState>,
) -> Result<Json<SystemConfigResponse>, ApiError> {
    let task_execution_max_iterations = state
        .task_service
        .effective_task_execution_max_iterations()
        .await
        .map_err(ApiError::bad_request)?;
    let tool_result_model_budget_limits = state
        .task_service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(system_config(
        &state.config,
        task_execution_max_iterations,
        tool_result_model_budget_limits,
    )))
}

async fn update_system_config_handler(
    State(state): State<AppState>,
    Json(input): Json<UpdateRuntimeSettingsRequest>,
) -> Result<Json<SystemConfigResponse>, ApiError> {
    let settings = state
        .task_service
        .update_runtime_settings(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(system_config(
        &state.config,
        settings.task_execution_max_iterations,
        chatos_ai_runtime::ToolResultModelBudgetLimits::new(
            settings.tool_result_model_max_chars,
            settings.tool_results_model_total_max_chars,
        ),
    )))
}

#[derive(Debug, Deserialize)]
struct TaskRunnerSkillQuery {
    lang: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskRunnerSkillResponse {
    name: &'static str,
    locale: &'static str,
    content: &'static str,
}

async fn task_runner_skill_handler(
    Query(query): Query<TaskRunnerSkillQuery>,
) -> Json<TaskRunnerSkillResponse> {
    let lang = query.lang.as_deref().unwrap_or("zh-CN").trim();
    let english = matches!(
        lang.to_ascii_lowercase().as_str(),
        "en" | "en-us" | "english"
    );
    Json(if english {
        TaskRunnerSkillResponse {
            name: "task-runner-ai-agent-en-us",
            locale: "en-US",
            content: TASK_RUNNER_SKILL_EN_US,
        }
    } else {
        TaskRunnerSkillResponse {
            name: "task-runner-ai-agent-zh-cn",
            locale: "zh-CN",
            content: TASK_RUNNER_SKILL_ZH_CN,
        }
    })
}

async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let current_user = current_user_from_request(&request, &state)?;
    let path = request.uri().path();
    if !current_user.is_admin() && path != "/api/auth/me" && path != "/api/auth/logout" {
        return Err(ApiError::forbidden("当前账号不能访问管理后台接口"));
    }
    request.extensions_mut().insert(current_user);
    Ok(next.run(request).await)
}

async fn login_handler(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let response = state
        .auth_service
        .login(input.username.as_str(), input.password.as_str())
        .await
        .map_err(ApiError::unauthorized)?;
    Ok(Json(response))
}

async fn agent_token_handler(
    State(state): State<AppState>,
    Json(input): Json<AgentTokenRequest>,
) -> Result<Json<AgentTokenResponse>, ApiError> {
    let response = state
        .auth_service
        .issue_agent_token(input.username.as_str(), input.password.as_str())
        .await
        .map_err(ApiError::unauthorized)?;
    Ok(Json(response))
}

async fn current_user_handler(
    Extension(current_user): Extension<CurrentUser>,
) -> Json<CurrentUserResponse> {
    Json(CurrentUserResponse {
        user: current_user.public_user(),
    })
}

async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = bearer_token_from_headers(&headers).map_err(ApiError::unauthorized)?;
    state.auth_service.logout(token);
    Ok(StatusCode::NO_CONTENT)
}

async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserSummaryRecord>>, ApiError> {
    let users = state
        .auth_service
        .list_users()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(users))
}

async fn create_user(
    State(state): State<AppState>,
    Json(input): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserSummaryRecord>), ApiError> {
    let user = state
        .auth_service
        .create_user(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_user(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateUserRequest>,
) -> Result<Json<UserSummaryRecord>, ApiError> {
    let user = state
        .auth_service
        .update_user(&id, input, &current_user)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("用户不存在: {id}")))?;
    Ok(Json(user))
}

async fn delete_user(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    if state
        .auth_service
        .delete_user(&id, &current_user)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("用户不存在: {id}")))
    }
}

fn current_user_from_request(request: &Request, state: &AppState) -> Result<CurrentUser, ApiError> {
    let token = bearer_token_from_request(request).map_err(ApiError::unauthorized)?;
    state
        .auth_service
        .current_user_for_token(token)
        .ok_or_else(|| ApiError::unauthorized("登录已失效，请重新登录"))
}

fn bearer_token_from_request(request: &Request) -> Result<&str, String> {
    bearer_token_from_headers(request.headers()).or_else(|_| {
        token_from_query(request.uri().query()).ok_or_else(|| "缺少登录令牌".to_string())
    })
}

fn bearer_token_from_headers(headers: &HeaderMap) -> Result<&str, String> {
    let value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| "缺少登录令牌".to_string())?
        .to_str()
        .map_err(|_| "登录令牌格式不正确".to_string())?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next().unwrap_or_default();
    let token = parts.next().unwrap_or_default();
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || parts.next().is_some() {
        return Err("登录令牌格式不正确".to_string());
    }
    Ok(token)
}

fn token_from_query(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?.trim();
        ((key == "access_token" || key == "token") && !value.is_empty()).then_some(value)
    })
}

#[derive(Debug, Default, Deserialize)]
struct TaskListQuery {
    status: Option<TaskStatus>,
    keyword: Option<String>,
    tag: Option<String>,
    model_config_id: Option<String>,
    scheduled_only: Option<bool>,
    parent_task_id: Option<String>,
    source_run_id: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Vec<TaskRecord>>, ApiError> {
    let tasks = state
        .task_service
        .list_tasks_filtered(TaskListFilters {
            status: query.status,
            keyword: query.keyword,
            tag: query.tag,
            model_config_id: query.model_config_id,
            creator_user_id: None,
            scheduled_only: query.scheduled_only,
            parent_task_id: query.parent_task_id,
            source_run_id: query.source_run_id,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(tasks))
}

async fn list_tasks_page(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<PaginatedResponse<TaskRecord>>, ApiError> {
    let page = state
        .task_service
        .list_tasks_page(TaskListFilters {
            status: query.status,
            keyword: query.keyword,
            tag: query.tag,
            model_config_id: query.model_config_id,
            creator_user_id: None,
            scheduled_only: query.scheduled_only,
            parent_task_id: query.parent_task_id,
            source_run_id: query.source_run_id,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page))
}

#[derive(Debug, Default, Deserialize)]
struct TaskSummaryQuery {
    ids: Option<String>,
    keyword: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<usize>,
}

async fn list_task_summaries(
    State(state): State<AppState>,
    Query(query): Query<TaskSummaryQuery>,
) -> Result<Json<Vec<TaskSummaryRecord>>, ApiError> {
    let summaries = if let Some(ids) = query.ids {
        state
            .task_service
            .get_task_summaries_by_ids(parse_csv_ids(&ids))
            .await
    } else {
        state
            .task_service
            .list_task_summaries_filtered(TaskListFilters {
                status: query.status,
                keyword: query.keyword,
                creator_user_id: None,
                limit: query.limit,
                ..TaskListFilters::default()
            })
            .await
    }
    .map_err(ApiError::bad_request)?;
    Ok(Json(summaries))
}

async fn get_task_index(
    State(state): State<AppState>,
) -> Result<Json<TaskIndexResponse>, ApiError> {
    let index = state
        .task_service
        .task_index()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(index))
}

async fn create_task(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskRecord>), ApiError> {
    let task = state
        .task_service
        .create_task(input, Some(&current_user), None)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn get_task_stats(
    State(state): State<AppState>,
) -> Result<Json<TaskStatsResponse>, ApiError> {
    let stats = state
        .task_service
        .task_stats()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(stats))
}

async fn batch_update_task_status(
    State(state): State<AppState>,
    Json(input): Json<BatchTaskStatusUpdateRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    let result = state
        .task_service
        .batch_update_status(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

async fn batch_delete_tasks(
    State(state): State<AppState>,
    Json(input): Json<BatchTaskDeleteRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    let result = state
        .task_service
        .batch_delete_tasks(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

async fn batch_start_task_runs(
    State(state): State<AppState>,
    Json(input): Json<BatchTaskRunRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    let result = state
        .run_service
        .batch_start_runs(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

async fn get_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TaskRecord>, ApiError> {
    state
        .task_service
        .get_task(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))
}

async fn update_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateTaskRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .update_task(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(task))
}

async fn delete_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    if state
        .task_service
        .delete_task(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("任务不存在: {id}")))
    }
}

async fn update_task_mcp(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateTaskMcpRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .update_task_mcp(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(task))
}

async fn list_task_prerequisites(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskSummaryRecord>>, ApiError> {
    let tasks = state
        .task_service
        .list_task_prerequisites(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(tasks))
}

async fn set_task_prerequisites(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<SetTaskPrerequisitesRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .set_task_prerequisites(&id, input.prerequisite_task_ids, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(task))
}

async fn get_task_dependency_graph(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TaskDependencyGraph>, ApiError> {
    let graph = state
        .task_service
        .get_task_dependency_graph(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(graph))
}

async fn record_task_process(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<RecordTaskProcessRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .record_task_process(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(task))
}

async fn preview_task_mcp_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<McpPromptPreviewResponse>, ApiError> {
    let preview = state
        .mcp_catalog_service
        .preview_task_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(preview))
}

#[derive(Debug, Default, Deserialize)]
struct TaskMemoryContextQuery {
    include_recent_records: Option<bool>,
    include_thread_summary: Option<bool>,
    include_subject_memory: Option<bool>,
    recent_record_limit: Option<usize>,
    summary_limit: Option<usize>,
}

async fn get_task_memory_context(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<TaskMemoryContextQuery>,
) -> Result<Json<TaskMemoryContextResponse>, ApiError> {
    let response = state
        .task_service
        .get_task_memory_context(
            &id,
            TaskMemoryContextOptions {
                include_recent_records: query.include_recent_records,
                include_thread_summary: query.include_thread_summary,
                include_subject_memory: query.include_subject_memory,
                recent_record_limit: query.recent_record_limit,
                summary_limit: query.summary_limit,
            },
        )
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(response))
}

#[derive(Debug, Default, Deserialize)]
struct TaskMemoryRecordsQuery {
    role: Option<String>,
    record_type: Option<String>,
    summary_status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    order: Option<String>,
}

async fn get_task_memory_records(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<TaskMemoryRecordsQuery>,
) -> Result<Json<TaskMemoryRecordsResponse>, ApiError> {
    let response = state
        .task_service
        .get_task_memory_records(
            &id,
            TaskMemoryRecordsOptions {
                role: query.role,
                record_type: query.record_type,
                summary_status: query.summary_status,
                limit: query.limit,
                offset: query.offset,
                order: query.order,
            },
        )
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(response))
}

async fn summarize_task_memory(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TaskMemorySummaryResponse>, ApiError> {
    let response = state
        .task_service
        .summarize_task_memory(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(response))
}

async fn list_model_configs(
    State(state): State<AppState>,
) -> Result<Json<Vec<ModelConfigRecord>>, ApiError> {
    let models = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(models))
}

async fn create_model_config(
    State(state): State<AppState>,
    Json(input): Json<CreateModelConfigRequest>,
) -> Result<(StatusCode, Json<ModelConfigRecord>), ApiError> {
    let model = state
        .model_config_service
        .create_model_config(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(model)))
}

async fn get_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ModelConfigRecord>, ApiError> {
    state
        .model_config_service
        .get_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))
}

async fn update_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateModelConfigRequest>,
) -> Result<Json<ModelConfigRecord>, ApiError> {
    let model = state
        .model_config_service
        .update_model_config(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(model))
}

async fn delete_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    if state
        .model_config_service
        .delete_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("模型配置不存在: {id}")))
    }
}

async fn test_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<TestModelConfigRequest>,
) -> Result<Json<ModelConfigTestResponse>, ApiError> {
    let result = state
        .model_config_service
        .test_model_config(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(result))
}

async fn list_model_catalog(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ModelCatalogResponse>, ApiError> {
    let result = state
        .model_config_service
        .list_model_catalog(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(result))
}

async fn preview_model_catalog(
    State(state): State<AppState>,
    Json(input): Json<PreviewModelCatalogRequest>,
) -> Result<Json<ModelCatalogResponse>, ApiError> {
    let result = state
        .model_config_service
        .preview_model_catalog(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

async fn list_model_config_usage(
    State(state): State<AppState>,
) -> Result<Json<Vec<ModelConfigUsageRecord>>, ApiError> {
    let usage = state
        .model_config_service
        .usage_stats()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(usage))
}

async fn list_remote_servers(
    State(state): State<AppState>,
) -> Result<Json<Vec<RemoteServerRecord>>, ApiError> {
    let servers = state
        .remote_server_service
        .list_remote_servers()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(servers))
}

async fn create_remote_server(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateRemoteServerRequest>,
) -> Result<(StatusCode, Json<RemoteServerRecord>), ApiError> {
    let server = state
        .remote_server_service
        .create_remote_server(input, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(server)))
}

async fn get_remote_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<RemoteServerRecord>, ApiError> {
    state
        .remote_server_service
        .get_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))
}

async fn update_remote_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateRemoteServerRequest>,
) -> Result<Json<RemoteServerRecord>, ApiError> {
    let server = state
        .remote_server_service
        .update_remote_server(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    Ok(Json(server))
}

async fn delete_remote_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    if state
        .remote_server_service
        .delete_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("远程服务器不存在: {id}")))
    }
}

async fn test_remote_server_draft(
    State(state): State<AppState>,
    Json(input): Json<TestRemoteServerRequest>,
) -> Result<Json<RemoteServerTestResponse>, ApiError> {
    let result = state
        .remote_server_service
        .test_remote_server_draft(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

async fn test_remote_server_saved(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<RemoteServerTestResponse>, ApiError> {
    let result = state
        .remote_server_service
        .test_remote_server_saved(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    Ok(Json(result))
}

async fn start_task_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<StartTaskRunRequest>,
) -> Result<(StatusCode, Json<TaskRunRecord>), ApiError> {
    let run = state
        .run_service
        .start_run(&id, input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(run)))
}

#[derive(Debug, Default, Deserialize)]
struct RunListQuery {
    task_id: Option<String>,
    status: Option<TaskRunStatus>,
    model_config_id: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_task_runs(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<TaskRunRecord>>, ApiError> {
    let runs = state
        .run_service
        .list_runs_filtered(RunListFilters {
            task_id: Some(id),
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(runs))
}

async fn list_runs(
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<TaskRunRecord>>, ApiError> {
    let runs = state
        .run_service
        .list_runs_filtered(RunListFilters {
            task_id: query.task_id,
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(runs))
}

async fn list_runs_page(
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<PaginatedResponse<TaskRunRecord>>, ApiError> {
    let page = state
        .run_service
        .list_runs_page(RunListFilters {
            task_id: query.task_id,
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page))
}

async fn list_run_index(
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Result<Json<Vec<RunSummaryRecord>>, ApiError> {
    let runs = state
        .run_service
        .run_index(RunListFilters {
            task_id: query.task_id,
            status: query.status,
            model_config_id: query.model_config_id,
            keyword: query.keyword,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(runs))
}

#[derive(Debug, Default, Deserialize)]
struct RunSummaryQuery {
    ids: Option<String>,
    task_id: Option<String>,
    status: Option<TaskRunStatus>,
    model_config_id: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
}

async fn list_run_summaries(
    State(state): State<AppState>,
    Query(query): Query<RunSummaryQuery>,
) -> Result<Json<Vec<RunSummaryRecord>>, ApiError> {
    let summaries = if let Some(ids) = query.ids {
        state
            .run_service
            .get_run_summaries_by_ids(parse_csv_ids(&ids))
            .await
    } else {
        state
            .run_service
            .run_index(RunListFilters {
                task_id: query.task_id,
                status: query.status,
                model_config_id: query.model_config_id,
                keyword: query.keyword,
                limit: query.limit,
                offset: None,
            })
            .await
    }
    .map_err(ApiError::bad_request)?;
    Ok(Json(summaries))
}

async fn get_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TaskRunRecord>, ApiError> {
    state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))
}

async fn list_run_events(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskRunEventRecord>>, ApiError> {
    let events = state
        .run_service
        .list_run_events(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(events))
}

async fn list_run_prompts(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<Vec<UiPromptRecord>>, ApiError> {
    let page = state
        .ui_prompt_service
        .list_prompts_page(PromptListFilters {
            task_id: query.task_id,
            run_id: Some(id),
            status: query.status,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page.items))
}

async fn cancel_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TaskRunRecord>, ApiError> {
    let run = state
        .run_service
        .cancel_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    Ok(Json(run))
}

async fn retry_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<TaskRunRecord>), ApiError> {
    let run = state
        .run_service
        .retry_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    Ok((StatusCode::CREATED, Json(run)))
}

async fn stream_run_events(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let seen_event_ids = match state.run_service.list_run_events(&id).await {
        Ok(events) => events
            .into_iter()
            .map(|event| event.id)
            .collect::<HashSet<_>>(),
        Err(err) => {
            tracing::warn!(
                "failed to initialize run event stream cache for {}: {}",
                id,
                err
            );
            HashSet::new()
        }
    };
    let stream = stream::unfold(
        RunEventStreamState {
            run_id: id,
            run_service: state.run_service.clone(),
            receiver: state.run_service.subscribe_run_events(),
            seen_event_ids,
            pending_events: VecDeque::new(),
            receiver_closed: false,
        },
        |mut stream_state| async move {
            loop {
                if let Some(event) = stream_state.pending_events.pop_front() {
                    return Some((Ok(run_event_sse_event(&event)), stream_state));
                }

                if stream_state.receiver_closed {
                    tokio::time::sleep(RUN_EVENT_POLL_INTERVAL).await;
                } else {
                    tokio::select! {
                        received = stream_state.receiver.recv() => {
                            match received {
                                Ok(event) => {
                                    if event.run_id == stream_state.run_id
                                        && stream_state.seen_event_ids.insert(event.id.clone())
                                    {
                                        return Some((Ok(run_event_sse_event(&event)), stream_state));
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    stream_state.receiver_closed = true;
                                }
                            }
                        }
                        _ = tokio::time::sleep(RUN_EVENT_POLL_INTERVAL) => {}
                    }
                }

                match stream_state
                    .run_service
                    .list_run_events(&stream_state.run_id)
                    .await
                {
                    Ok(events) => {
                        let mut unseen = events
                            .into_iter()
                            .filter(|event| stream_state.seen_event_ids.insert(event.id.clone()))
                            .collect::<Vec<_>>();
                        unseen.sort_by(|left, right| {
                            left.created_at
                                .cmp(&right.created_at)
                                .then(left.id.cmp(&right.id))
                        });
                        stream_state.pending_events.extend(unseen);
                    }
                    Err(err) => {
                        tracing::warn!(
                            "failed to poll run events for {}: {}",
                            stream_state.run_id,
                            err
                        );
                    }
                }
            }
        },
    );
    Sse::new(stream).keep_alive(KeepAlive::default())
}

struct RunEventStreamState {
    run_id: String,
    run_service: crate::services::RunService,
    receiver: tokio::sync::broadcast::Receiver<TaskRunEventRecord>,
    seen_event_ids: HashSet<String>,
    pending_events: VecDeque<TaskRunEventRecord>,
    receiver_closed: bool,
}

fn run_event_sse_event(event: &TaskRunEventRecord) -> Event {
    Event::default()
        .event("run_event")
        .data(serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string()))
}

#[derive(Debug, Default, Deserialize)]
struct PromptListQuery {
    task_id: Option<String>,
    run_id: Option<String>,
    status: Option<UiPromptStatus>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct PromptTaskCountQuery {
    status: Option<UiPromptStatus>,
}

async fn list_prompts(
    State(state): State<AppState>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<Vec<UiPromptRecord>>, ApiError> {
    let page = state
        .ui_prompt_service
        .list_prompts_page(PromptListFilters {
            task_id: query.task_id,
            run_id: query.run_id,
            status: query.status,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page.items))
}

async fn list_prompts_page(
    State(state): State<AppState>,
    Query(query): Query<PromptListQuery>,
) -> Result<Json<PaginatedResponse<UiPromptRecord>>, ApiError> {
    let page = state
        .ui_prompt_service
        .list_prompts_page(PromptListFilters {
            task_id: query.task_id,
            run_id: query.run_id,
            status: query.status,
            limit: query.limit,
            offset: query.offset,
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(page))
}

async fn list_prompt_task_counts(
    State(state): State<AppState>,
    Query(query): Query<PromptTaskCountQuery>,
) -> Result<Json<Vec<UiPromptTaskCountRecord>>, ApiError> {
    let counts = state
        .ui_prompt_service
        .list_prompt_task_counts(query.status)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(counts))
}

async fn get_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    state
        .ui_prompt_service
        .get_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))
}

async fn submit_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<SubmitUiPromptRequest>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    let prompt = state
        .ui_prompt_service
        .submit_prompt(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))?;
    Ok(Json(prompt))
}

async fn cancel_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<CancelUiPromptRequest>,
) -> Result<Json<UiPromptRecord>, ApiError> {
    let prompt = state
        .ui_prompt_service
        .cancel_prompt(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("提示不存在: {id}")))?;
    Ok(Json(prompt))
}

fn parse_csv_ids(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .take(200)
        .collect()
}

#[derive(Debug, Default, Deserialize)]
struct ToolingNotepadQuery {
    user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ToolingNotepadNotesQuery {
    user_id: Option<String>,
    folder: Option<String>,
    tags: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
    match_any: Option<bool>,
    recursive: Option<bool>,
}

async fn list_notepad_folders(
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .list_notepad_folders(query.user_id.as_deref())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn list_notepad_tags(
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .list_notepad_tags(query.user_id.as_deref())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn list_notepad_notes(
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadNotesQuery>,
) -> Result<Json<Value>, ApiError> {
    let tags = query
        .tags
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let response = state
        .tooling_state_service
        .list_notepad_notes(
            query.user_id.as_deref(),
            query.folder,
            tags,
            query.query,
            query.limit,
            query.match_any.unwrap_or(false),
            query.recursive.unwrap_or(true),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn read_notepad_note(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .read_notepad_note(query.user_id.as_deref(), &id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

#[derive(Debug, Default, Deserialize)]
struct ToolingTerminalProcessesQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    include_exited: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct ToolingTerminalLogsQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct KillTerminalProcessRequest {
    user_id: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WriteTerminalProcessRequest {
    user_id: Option<String>,
    project_id: Option<String>,
    data: String,
    submit: Option<bool>,
}

async fn list_terminal_processes(
    State(state): State<AppState>,
    Query(query): Query<ToolingTerminalProcessesQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .list_terminal_processes(
            query.user_id,
            query.project_id,
            query.include_exited.unwrap_or(true),
            query.limit.unwrap_or(50),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn get_terminal_process_logs(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ToolingTerminalLogsQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .get_terminal_process_logs(
            &id,
            query.user_id,
            query.project_id,
            query.offset,
            query.limit,
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn kill_terminal_process(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<KillTerminalProcessRequest>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .kill_terminal_process(&id, input.user_id, input.project_id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn write_terminal_process(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<WriteTerminalProcessRequest>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .write_terminal_process(
            &id,
            input.user_id,
            input.project_id,
            input.data,
            input.submit.unwrap_or(true),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn list_mcp_catalog(State(state): State<AppState>) -> Json<Vec<McpCatalogEntry>> {
    Json(state.mcp_catalog_service.list_catalog())
}

async fn get_mcp_server_info(State(state): State<AppState>) -> Json<McpServerInfo> {
    Json(state.task_runner_mcp_service.server_info())
}

async fn preview_mcp_prompt(
    State(state): State<AppState>,
    Json(input): Json<McpPromptPreviewRequest>,
) -> Result<Json<McpPromptPreviewResponse>, ApiError> {
    let preview = state
        .mcp_catalog_service
        .preview_prompt(input)
        .map_err(ApiError::bad_request)?;
    Ok(Json(preview))
}

async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let current_user = bearer_token_from_headers(&headers)
        .map_err(ApiError::unauthorized)
        .and_then(|token| {
            state
                .auth_service
                .current_user_for_token(token)
                .ok_or_else(|| ApiError::unauthorized("登录已失效，请重新登录"))
        });
    let current_user = match current_user {
        Ok(value) => value,
        Err(err) => {
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(crate::mcp_server::JsonRpcError {
                    code: -32001,
                    message: err.message,
                }),
            });
        }
    };
    Json(
        state
            .task_runner_mcp_service
            .handle_jsonrpc(
                request,
                current_user,
                mcp_request_context_from_headers(&headers),
            )
            .await,
    )
}

fn mcp_request_context_from_headers(headers: &HeaderMap) -> McpRequestContext {
    McpRequestContext {
        source_session_id: header_text(headers, "x-chatos-session-id")
            .or_else(|| header_text(headers, "x-chatos-conversation-id")),
        source_turn_id: header_text(headers, "x-chatos-turn-id"),
        source_user_message_id: header_text(headers, "x-chatos-user-message-id"),
        workspace_dir: header_text(headers, "x-task-runner-workspace-dir")
            .or_else(|| header_text(headers, "x-chatos-workspace-dir"))
            .or_else(|| header_text(headers, "x-chatos-workspace-root")),
        remote_server_config: header_text(headers, "x-task-runner-remote-server-config")
            .or_else(|| header_text(headers, "x-task-runner-remote-server-json")),
        tool_profile: header_text(headers, "x-task-runner-tool-profile"),
    }
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

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
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorBody {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}
