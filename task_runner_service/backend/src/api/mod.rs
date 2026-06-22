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
    AgentTokenRequest, AgentTokenResponse, BatchTaskDeleteRequest, BatchTaskOperationResponse,
    BatchTaskRunRequest, BatchTaskStatusUpdateRequest, CancelTaskRequest, CancelTaskResponse,
    CancelUiPromptRequest, CreateExternalMcpConfigRequest, CreateModelConfigRequest,
    CreateRemoteServerRequest, CreateTaskRequest, CreateUserRequest, CurrentUserResponse,
    ExternalMcpConfigRecord, HealthResponse, LoginRequest, LoginResponse, McpCatalogEntry,
    McpPromptPreviewRequest, McpPromptPreviewResponse, McpServerInfo, ModelCatalogResponse,
    ModelConfigRecord, ModelConfigTestResponse, ModelConfigUsageRecord, PaginatedResponse,
    PreviewModelCatalogRequest, PromptListFilters, RecordTaskProcessRequest, RemoteServerRecord,
    RemoteServerTestResponse, RunListFilters, RunSummaryRecord, SetTaskPrerequisitesRequest,
    StartTaskRunRequest, SubmitUiPromptRequest, SystemConfigResponse, TaskDependencyGraph,
    TaskIndexResponse, TaskListFilters, TaskMemoryContextOptions, TaskMemoryContextResponse,
    TaskMemoryRecordsOptions, TaskMemoryRecordsResponse, TaskMemorySummaryResponse, TaskRecord,
    TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskRunnerInternalPromptPreviewResponse,
    TaskStatsResponse, TaskStatus, TaskSummaryRecord, TestModelConfigRequest,
    TestRemoteServerRequest, UiPromptRecord, UiPromptStatus, UiPromptTaskCountRecord,
    UpdateExternalMcpConfigRequest, UpdateModelConfigRequest, UpdateRemoteServerRequest,
    UpdateRuntimeSettingsRequest, UpdateTaskMcpRequest, UpdateTaskRequest, UpdateUserRequest,
    UserSummaryRecord,
};
use crate::services::{health, system_config};
use crate::state::AppState;

mod chatos_internal;
mod core;
mod external_mcp_configs;
mod mcp;
mod models;
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
