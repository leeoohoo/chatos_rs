use std::collections::{HashSet, VecDeque};

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::CurrentUser;
use crate::mcp_server::{JsonRpcRequest, JsonRpcResponse, McpRequestContext};
use crate::models::{
    normalize_project_id, AgentTokenRequest, AgentTokenResponse, BatchTaskDeleteRequest,
    BatchTaskOperationItem, BatchTaskOperationResponse, BatchTaskRunRequest,
    BatchTaskStatusUpdateRequest, CancelTaskRequest, CancelTaskResponse, CancelUiPromptRequest,
    ChatosProjectImportRequest, CreateExternalMcpConfigRequest, CreateModelConfigRequest,
    CreateRemoteServerRequest, CreateTaskProjectRequest, CreateTaskRequest, CreateUserRequest,
    CurrentUserResponse, ExternalMcpConfigRecord, HealthResponse, LoginRequest, LoginResponse,
    McpCatalogEntry, McpPromptPreviewRequest, McpPromptPreviewResponse, McpServerInfo,
    ModelCatalogResponse, ModelConfigRecord, ModelConfigTestResponse, ModelConfigUsageRecord,
    PaginatedResponse, PreviewModelCatalogRequest, PromptListFilters, RecordTaskProcessRequest,
    RemoteServerRecord, RemoteServerTestResponse, RunListFilters, RunSummaryRecord,
    SetTaskPrerequisitesRequest, StartTaskRunRequest, SubmitUiPromptRequest, SystemConfigResponse,
    TaskDependencyGraph, TaskIndexResponse, TaskListFilters, TaskMemoryContextOptions,
    TaskMemoryContextResponse, TaskMemoryRecordsOptions, TaskMemoryRecordsResponse,
    TaskMemorySummaryResponse, TaskProjectRecord, TaskProjectStatus, TaskRecord,
    TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskRunnerInternalPromptPreviewResponse,
    TaskScheduleMode, TaskStatsResponse, TaskStatus, TaskSummaryRecord, TestModelConfigRequest,
    TestRemoteServerRequest, UiPromptRecord, UiPromptStatus, UiPromptTaskCountRecord,
    UpdateExternalMcpConfigRequest, UpdateModelConfigRequest, UpdateRemoteServerRequest,
    UpdateRuntimeSettingsRequest, UpdateTaskMcpRequest, UpdateTaskProjectRequest,
    UpdateTaskRequest, UpdateUserRequest, UserRole, UserSummaryRecord, PUBLIC_PROJECT_ID,
};
use crate::services::{health, system_config};
use crate::state::AppState;

mod chatos_internal;
mod core;
mod external_mcp_configs;
mod mcp;
mod models;
mod projects;
mod prompts;
mod remote_servers;
mod router;
mod runs;
mod tasks;
mod tooling;

pub use self::router::build_router;

const RUN_EVENT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(750);
const TASK_RUNNER_SKILL_ZH_CN: &str = include_str!("../../../TASK_RUNNER_AI_SKILL.zh-CN.md");
const TASK_RUNNER_SKILL_EN_US: &str = include_str!("../../../TASK_RUNNER_AI_SKILL.en-US.md");

fn parse_csv_ids(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .take(200)
        .collect()
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

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    fn into_message(self) -> String {
        self.message
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

fn require_admin_user(current_user: &CurrentUser) -> Result<(), ApiError> {
    if current_user.is_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden("当前账号没有管理员权限"))
    }
}

fn effective_owner_user_id(current_user: &CurrentUser) -> Result<String, ApiError> {
    current_user
        .effective_owner_user_id()
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::unauthorized("当前登录态缺少用户归属信息"))
}

fn task_filters_for_user(
    mut filters: TaskListFilters,
    current_user: &CurrentUser,
) -> Result<TaskListFilters, ApiError> {
    if !current_user.is_admin() {
        filters.creator_user_id = Some(effective_owner_user_id(current_user)?);
    }
    Ok(filters)
}

fn owned_resource_visible_to_user(
    owner_user_id: Option<&str>,
    current_user: &CurrentUser,
) -> Result<bool, ApiError> {
    if current_user.is_admin() {
        return Ok(true);
    }
    let owner_user_id = owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let expected_owner_user_id = effective_owner_user_id(current_user)?;
    Ok(owner_user_id == Some(expected_owner_user_id.as_str()))
}

fn resource_owner_or_creator<'a>(
    owner_user_id: Option<&'a str>,
    creator_user_id: Option<&'a str>,
) -> Option<&'a str> {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            creator_user_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

fn ensure_owned_resource_access(
    owner_user_id: Option<&str>,
    current_user: &CurrentUser,
) -> Result<(), ApiError> {
    if owned_resource_visible_to_user(owner_user_id, current_user)? {
        Ok(())
    } else {
        Err(ApiError::forbidden("无权访问该资源"))
    }
}

fn ensure_task_access(task: &TaskRecord, current_user: &CurrentUser) -> Result<(), ApiError> {
    ensure_owned_resource_access(
        resource_owner_or_creator(
            task.owner_user_id.as_deref(),
            task.creator_user_id.as_deref(),
        ),
        current_user,
    )
}

async fn get_task_for_user(
    state: &AppState,
    id: &str,
    current_user: &CurrentUser,
) -> Result<Option<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .get_task(id)
        .await
        .map_err(ApiError::bad_request)?;
    match task {
        Some(task) if ensure_task_access(&task, current_user).is_ok() => Ok(Some(task)),
        Some(_) => Err(ApiError::forbidden("无权访问该任务")),
        None => Ok(None),
    }
}

async fn visible_task_ids_for_user(
    state: &AppState,
    current_user: &CurrentUser,
) -> Result<Option<HashSet<String>>, ApiError> {
    if current_user.is_admin() {
        return Ok(None);
    }
    let tasks = state
        .task_service
        .list_task_summaries_filtered(TaskListFilters {
            creator_user_id: Some(effective_owner_user_id(current_user)?),
            ..TaskListFilters::default()
        })
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Some(tasks.into_iter().map(|task| task.id).collect()))
}

async fn ensure_run_access(
    state: &AppState,
    run: &TaskRunRecord,
    current_user: &CurrentUser,
) -> Result<(), ApiError> {
    get_task_for_user(state, run.task_id.as_str(), current_user)
        .await?
        .map(|_| ())
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {}", run.task_id)))
}

fn task_stats_from_tasks(tasks: &[TaskRecord]) -> TaskStatsResponse {
    let mut stats = TaskStatsResponse {
        total: 0,
        scheduled: 0,
        follow_up: 0,
        draft: 0,
        ready: 0,
        queued: 0,
        running: 0,
        succeeded: 0,
        failed: 0,
        blocked: 0,
        cancelled: 0,
        archived: 0,
    };
    for task in tasks {
        stats.total += 1;
        if !matches!(task.schedule.mode, TaskScheduleMode::Manual) {
            stats.scheduled += 1;
        }
        if task.parent_task_id.is_some() {
            stats.follow_up += 1;
        }
        match task.status {
            TaskStatus::Draft => stats.draft += 1,
            TaskStatus::Ready => stats.ready += 1,
            TaskStatus::Queued => stats.queued += 1,
            TaskStatus::Running => stats.running += 1,
            TaskStatus::Succeeded => stats.succeeded += 1,
            TaskStatus::Failed => stats.failed += 1,
            TaskStatus::Blocked => stats.blocked += 1,
            TaskStatus::Cancelled => stats.cancelled += 1,
            TaskStatus::Archived => stats.archived += 1,
        }
    }
    stats
}
