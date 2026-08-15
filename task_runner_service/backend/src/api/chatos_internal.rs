// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chatos_project_execution::{
    STATUS_ALREADY_CONFIRMED, STATUS_AWAITING_CONFIRMATION, STATUS_EXECUTION_STARTED, STATUS_PAUSED,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    models::{ModelConfigRecord, TaskRunRecord, TaskRunStatus, TaskStatus},
    services::{
        ChatosMessageModelConfigSummary, ChatosMessageRunDetail, ChatosMessageTaskRun,
        ChatosMessageTaskRunEvent, ChatosMessageTaskSummary,
    },
    state::AppState,
};

use super::internal_auth::{
    require_task_runner_internal_request, TaskRunnerInternalAuditGuard,
    TaskRunnerInternalRequestIdentity, CHATOS_CALLER, CHATOS_EXECUTION_START_SCOPE,
    CHATOS_MESSAGES_READ_SCOPE, CHATOS_MODELS_READ_SCOPE, CHATOS_MODELS_RUNTIME_SCOPE,
};
mod project_execution;
mod projection;
use project_execution::{
    clone_chatos_project_execution, confirm_chatos_project_execution,
    pause_chatos_project_execution, require_chatos_execution_mutation, required_internal_text,
    resume_chatos_project_execution, retire_chatos_project_execution,
};
use projection::{
    paginate_run_events, redact_workspace_paths_internal, run_event_page,
    trim_event_for_chatos_detail, trim_run_for_chatos_detail,
};

const DEFAULT_RUN_EVENT_LIMIT: usize = 40;
const MAX_RUN_EVENT_LIMIT: usize = 100;
const RUN_SNAPSHOT_PREVIEW_LIMIT_BYTES: usize = 256 * 1024;
const RUN_EVENT_PAYLOAD_PREVIEW_LIMIT_BYTES: usize = 32 * 1024;
const RUN_EVENT_MESSAGE_PREVIEW_LIMIT_BYTES: usize = 16 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/chatos/message-tasks",
            get(list_chatos_message_tasks),
        )
        .route(
            "/internal/chatos/message-graph",
            get(get_chatos_message_graph),
        )
        .route(
            "/internal/chatos/message-tasks/{task_id}",
            get(get_chatos_message_task),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}",
            get(get_chatos_message_run),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/retry",
            post(retry_chatos_message_run),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/changes",
            get(get_chatos_message_run_changes),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/integration/retry",
            post(retry_chatos_message_run_integration),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/integration/waive",
            post(waive_chatos_message_run_integration),
        )
        .route(
            "/internal/chatos/message-runs/{run_id}/events/{event_id}",
            get(get_chatos_message_run_event),
        )
        .route(
            "/internal/chatos/message-graph/runs/{run_id}",
            get(get_chatos_message_graph_run),
        )
        .route(
            "/internal/chatos/session-active-message-tasks",
            post(list_chatos_session_active_message_tasks),
        )
        .route(
            "/internal/chatos/project-execution/confirm",
            post(confirm_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/pause",
            post(pause_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/resume",
            post(resume_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/clone",
            post(clone_chatos_project_execution),
        )
        .route(
            "/internal/chatos/project-execution/retire",
            post(retire_chatos_project_execution),
        )
        .route(
            "/internal/chatos/users/{owner_user_id}/model-configs",
            get(list_chatos_user_model_configs),
        )
        .route(
            "/internal/chatos/users/{owner_user_id}/model-configs/{model_config_id}/runtime",
            get(get_chatos_model_runtime_config),
        )
}

#[derive(Debug, Serialize)]
struct ChatosModelConfigCatalogItem {
    id: String,
    name: String,
    provider: String,
    prompt_vendor: Option<String>,
    base_url: String,
    model: String,
    thinking_level: Option<String>,
    task_usage_scenario: Option<String>,
    task_thinking_level: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    has_api_key: bool,
    enabled: bool,
    supports_images: bool,
    supports_reasoning: bool,
    supports_responses: bool,
    sync_warnings: Vec<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Default, Deserialize)]
struct ChatosModelConfigCatalogQuery {
    include_all: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ChatosModelRuntimeConfig {
    id: String,
    owner_user_id: Option<String>,
    name: String,
    provider: String,
    prompt_vendor: Option<String>,
    base_url: String,
    api_key: String,
    model: String,
    usage_scenario: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    model_request_max_retries: usize,
    thinking_level: Option<String>,
    supports_images: bool,
    supports_reasoning: bool,
    supports_responses: bool,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

impl From<ModelConfigRecord> for ChatosModelRuntimeConfig {
    fn from(model: ModelConfigRecord) -> Self {
        Self {
            id: model.id,
            owner_user_id: model.owner_user_id,
            name: model.name,
            provider: model.provider,
            prompt_vendor: model.prompt_vendor,
            base_url: model.base_url,
            api_key: model.api_key,
            model: model.model,
            usage_scenario: model.usage_scenario,
            temperature: model.temperature,
            max_output_tokens: model.max_output_tokens,
            model_request_max_retries: model.model_request_max_retries,
            thinking_level: model.thinking_level,
            supports_images: model.supports_images,
            supports_reasoning: model.supports_reasoning,
            supports_responses: model.supports_responses,
            enabled: model.enabled,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

impl From<ModelConfigRecord> for ChatosModelConfigCatalogItem {
    fn from(model: ModelConfigRecord) -> Self {
        Self {
            id: model.id,
            name: model.name,
            provider: model.provider,
            prompt_vendor: model.prompt_vendor,
            base_url: model.base_url,
            model: model.model,
            thinking_level: model.thinking_level,
            task_usage_scenario: model.usage_scenario,
            task_thinking_level: None,
            temperature: model.temperature,
            max_output_tokens: model.max_output_tokens,
            has_api_key: !model.api_key.trim().is_empty(),
            enabled: model.enabled,
            supports_images: model.supports_images,
            supports_reasoning: model.supports_reasoning,
            supports_responses: model.supports_responses,
            sync_warnings: Vec::new(),
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

async fn list_chatos_user_model_configs(
    Path(owner_user_id): Path<String>,
    Query(query): Query<ChatosModelConfigCatalogQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ChatosModelConfigCatalogItem>>, InternalApiError> {
    let identity = require_task_runner_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER],
        CHATOS_MODELS_READ_SCOPE,
    )
    .map_err(|err| InternalApiError {
        status: err.status,
        message: err.message,
    })?;
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err(InternalApiError::bad_request("owner_user_id is required"));
    }

    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        None,
        "model_config_catalog",
        owner_user_id,
        "list",
    );
    audit.represented_user_id(Some(owner_user_id));

    let models = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(InternalApiError::internal)?
        .into_iter()
        .filter(|model| {
            query.include_all.unwrap_or(false)
                || model
                    .owner_user_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    == Some(owner_user_id)
        })
        .map(ChatosModelConfigCatalogItem::from)
        .collect::<Vec<_>>();

    audit.succeeded();
    Ok(Json(models))
}

async fn get_chatos_model_runtime_config(
    Path((owner_user_id, model_config_id)): Path<(String, String)>,
    Query(query): Query<ChatosModelConfigCatalogQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ChatosModelRuntimeConfig>, InternalApiError> {
    let identity = require_task_runner_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER],
        CHATOS_MODELS_RUNTIME_SCOPE,
    )
    .map_err(|err| InternalApiError {
        status: err.status,
        message: err.message,
    })?;
    let owner_user_id = owner_user_id.trim();
    let model_config_id = model_config_id.trim();
    if owner_user_id.is_empty() || model_config_id.is_empty() {
        return Err(InternalApiError::bad_request(
            "owner_user_id and model_config_id are required",
        ));
    }

    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        None,
        "model_config_runtime",
        model_config_id,
        "read",
    );
    audit.represented_user_id(Some(owner_user_id));

    let model = state
        .model_config_service
        .get_model_config(model_config_id)
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("model config not found"))?;
    let owns_model = model
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(owner_user_id);
    if !owns_model && !query.include_all.unwrap_or(false) {
        return Err(InternalApiError {
            status: StatusCode::FORBIDDEN,
            message: "forbidden model config access".to_string(),
        });
    }

    audit.succeeded();
    Ok(Json(ChatosModelRuntimeConfig::from(model)))
}

#[derive(Debug, Deserialize)]
struct ChatosMessageTaskQuery {
    source_session_id: String,
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RetryChatosMessageRunRequest {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
    retry_instruction: Option<String>,
    execution_service_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RetryChatosMessageRunIntegrationRequest {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
}

#[derive(Debug, Deserialize)]
struct WaiveChatosMessageRunIntegrationRequest {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ChatosMessageRunQuery {
    #[serde(flatten)]
    source: ChatosMessageTaskQuery,
    event_limit: Option<usize>,
    event_offset: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ChatosMessageTasksResponse {
    items: Vec<ChatosMessageTaskSummary>,
}

#[derive(Debug, Deserialize)]
struct ChatosSessionActiveMessageTasksRequest {
    source_session_id: String,
    #[serde(default)]
    source_user_message_ids: Vec<String>,
    #[serde(default)]
    source_turn_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ChatosActiveMessageTaskSource {
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
    running_count: usize,
    active_count: usize,
}

#[derive(Debug, Serialize)]
struct ChatosSessionActiveMessageTasksResponse {
    source_session_id: String,
    active_source_user_message_ids: Vec<String>,
    running_source_user_message_ids: Vec<String>,
    items: Vec<ChatosActiveMessageTaskSource>,
}

#[derive(Debug, Deserialize)]
struct ConfirmChatosProjectExecutionRequest {
    project_id: String,
    requirement_id: String,
    source_session_id: String,
    source_user_message_id: String,
}

type MutateChatosProjectExecutionRequest = ConfirmChatosProjectExecutionRequest;

#[derive(Debug, Deserialize)]
struct CloneChatosProjectExecutionRequest {
    project_id: String,
    requirement_id: String,
    old_source_session_id: String,
    old_source_user_message_id: String,
    new_source_session_id: String,
    new_source_user_message_id: String,
}

#[derive(Debug, Deserialize)]
struct RetireChatosProjectExecutionRequest {
    project_id: String,
    requirement_id: String,
    source_session_id: String,
    source_user_message_id: String,
}

#[derive(Debug)]
pub(super) struct InternalApiError {
    status: StatusCode,
    message: String,
}

impl InternalApiError {
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

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
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

impl IntoResponse for InternalApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn validate_chatos_message_query(
    query: &ChatosMessageTaskQuery,
) -> Result<(&str, Option<&str>, Option<&str>), InternalApiError> {
    let source_session_id = query.source_session_id.trim();
    let source_user_message_id = query
        .source_user_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let source_turn_id = query
        .source_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source_session_id.is_empty()
        || (source_user_message_id.is_none() && source_turn_id.is_none())
    {
        return Err(InternalApiError::bad_request(
            "source_session_id and source_user_message_id or source_turn_id are required",
        ));
    }
    Ok((source_session_id, source_user_message_id, source_turn_id))
}

async fn list_chatos_message_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let items = state
        .task_service
        .list_message_task_summaries_for_chatos_source(
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?;
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageTasksResponse { items },
    )?))
}

async fn list_chatos_session_active_message_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatosSessionActiveMessageTasksRequest>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let source_session_id = request.source_session_id.trim();
    if source_session_id.is_empty() {
        return Err(InternalApiError::bad_request(
            "source_session_id is required",
        ));
    }
    let items = state
        .task_service
        .list_active_message_task_sources_for_chatos_session(
            source_session_id,
            request.source_user_message_ids.as_slice(),
            request.source_turn_ids.as_slice(),
        )
        .await
        .map_err(InternalApiError::internal)?;
    let active_source_user_message_ids = items
        .iter()
        .filter_map(|item| item.source_user_message_id.clone())
        .collect::<Vec<_>>();
    let running_source_user_message_ids = items
        .iter()
        .filter(|item| item.running_count > 0)
        .filter_map(|item| item.source_user_message_id.clone())
        .collect::<Vec<_>>();
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosSessionActiveMessageTasksResponse {
            source_session_id: source_session_id.to_string(),
            running_source_user_message_ids,
            active_source_user_message_ids,
            items: items
                .into_iter()
                .map(|item| ChatosActiveMessageTaskSource {
                    source_user_message_id: item.source_user_message_id,
                    source_turn_id: item.source_turn_id,
                    running_count: item.running_count,
                    active_count: item.active_count,
                })
                .collect(),
        },
    )?))
}

async fn get_chatos_message_task(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let detail = state
        .task_service
        .get_message_task_detail_for_chatos_source(
            task_id.trim(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("task not found for message"))?;
    Ok(Json(redact_workspace_paths_internal(&state, detail)?))
}

async fn get_chatos_message_graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let graph = state
        .task_service
        .get_message_task_graph_for_chatos_source(
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?;
    Ok(Json(redact_workspace_paths_internal(&state, graph)?))
}

async fn get_chatos_message_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageRunQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query.source)?;
    let (event_limit, event_offset) = run_event_page(&query);
    let run = state
        .run_service
        .get_run(run_id.trim())
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let task = state
        .task_service
        .get_message_task_detail_for_chatos_source(
            run.task_id.as_str(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let events = state
        .run_service
        .list_run_events(run.id.as_str())
        .await
        .map_err(InternalApiError::internal)?;
    let tool_text_limit_chars = state
        .task_service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(InternalApiError::internal)?
        .per_result_max_chars;
    let (events, events_total, events_has_more) =
        paginate_run_events(events, event_limit, event_offset, tool_text_limit_chars);
    let model_config = state
        .model_config_service
        .get_model_config(run.model_config_id.as_str())
        .await
        .map_err(InternalApiError::internal)?
        .map(ChatosMessageModelConfigSummary::from);
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageRunDetail {
            task,
            run: ChatosMessageTaskRun::from(trim_run_for_chatos_detail(run)),
            model_config,
            events,
            events_total,
            events_limit: event_limit,
            events_offset: event_offset,
            events_has_more,
        },
    )?))
}

async fn retry_chatos_message_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetryChatosMessageRunRequest>,
) -> Result<(StatusCode, Json<Value>), InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let run_id = required_internal_text(run_id, "run_id")?;
    let mut audit =
        TaskRunnerInternalAuditGuard::new(&identity, None, "task_run", run_id.as_str(), "retry");
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&request.source)?;
    let retry_instruction = request
        .retry_instruction
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let execution_service_id = request
        .execution_service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if retry_instruction.is_some_and(|value| value.chars().count() > 4000) {
        return Err(InternalApiError::bad_request(
            "retry_instruction must not exceed 4000 characters",
        ));
    }
    if execution_service_id.is_some_and(|value| value.chars().count() > 255) {
        return Err(InternalApiError::bad_request(
            "execution_service_id must not exceed 255 characters",
        ));
    }
    let run = require_chatos_message_run(
        &state,
        run_id.as_str(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    if let Ok(Some(task)) = state.task_service.get_task(run.task_id.as_str()).await {
        audit.represented_user_id(
            task.owner_user_id
                .as_deref()
                .or(task.creator_user_id.as_deref()),
        );
        audit.tenant_id(Some(task.tenant_id.as_str()));
        audit.project_id(Some(task.project_id.as_str()));
        audit.resource_name(Some(task.title.as_str()));
    }
    require_retryable_message_run(&run.status)?;
    let retried = state
        .run_service
        .retry_run_with_instruction_and_execution_service(
            run.id.as_str(),
            retry_instruction.map(ToOwned::to_owned),
            execution_service_id.map(ToOwned::to_owned),
        )
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let response = (
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "run": ChatosMessageTaskRun::from(retried),
        })),
    );
    audit.succeeded();
    Ok(response)
}

async fn retry_chatos_message_run_integration(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetryChatosMessageRunIntegrationRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let run_id = required_internal_text(run_id, "run_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        None,
        "task_run_integration",
        run_id.as_str(),
        "retry",
    );
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&request.source)?;
    let run = require_chatos_message_run(
        &state,
        run_id.as_str(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    if let Ok(Some(task)) = state.task_service.get_task(run.task_id.as_str()).await {
        audit.represented_user_id(
            task.owner_user_id
                .as_deref()
                .or(task.creator_user_id.as_deref()),
        );
        audit.tenant_id(Some(task.tenant_id.as_str()));
        audit.project_id(Some(task.project_id.as_str()));
        audit.resource_name(Some(task.title.as_str()));
    }
    let retried = state
        .run_service
        .retry_run_workspace_integration(run.id.as_str())
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| {
            InternalApiError::conflict("run does not have a retryable code integration conflict")
        })?;
    audit.succeeded();
    Ok(Json(json!({
        "success": true,
        "run": ChatosMessageTaskRun::from(retried),
    })))
}

async fn waive_chatos_message_run_integration(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<WaiveChatosMessageRunIntegrationRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let run_id = required_internal_text(run_id, "run_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        None,
        "task_run_integration",
        run_id.as_str(),
        "waive",
    );
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&request.source)?;
    let run = require_chatos_message_run(
        &state,
        run_id.as_str(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    if let Ok(Some(task)) = state.task_service.get_task(run.task_id.as_str()).await {
        audit.represented_user_id(
            task.owner_user_id
                .as_deref()
                .or(task.creator_user_id.as_deref()),
        );
        audit.tenant_id(Some(task.tenant_id.as_str()));
        audit.project_id(Some(task.project_id.as_str()));
        audit.resource_name(Some(task.title.as_str()));
    }
    let waived = state
        .run_service
        .waive_run_workspace_integration(run.id.as_str(), request.reason.as_str())
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| {
            InternalApiError::conflict("run does not have a waivable code integration conflict")
        })?;
    audit.succeeded();
    Ok(Json(json!({
        "success": true,
        "run": ChatosMessageTaskRun::from(waived),
    })))
}

async fn get_chatos_message_run_changes(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let run = require_chatos_message_run(
        &state,
        run_id.trim(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    let task = state
        .task_service
        .get_task(run.task_id.as_str())
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("task not found for message"))?;
    let changes = crate::services::load_task_run_workspace_changes(&state.run_service, &task, &run)
        .await
        .map_err(InternalApiError::bad_gateway)?;
    Ok(Json(redact_workspace_paths_internal(&state, changes)?))
}

fn require_retryable_message_run(status: &TaskRunStatus) -> Result<(), InternalApiError> {
    if matches!(status, TaskRunStatus::Failed | TaskRunStatus::Blocked) {
        return Ok(());
    }
    Err(InternalApiError::bad_request(
        "only a failed or blocked message task run can be retried",
    ))
}

async fn get_chatos_message_run_event(
    Path((run_id, event_id)): Path<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let run = require_chatos_message_run(
        &state,
        run_id.trim(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    let event_id = event_id.trim();
    if event_id.is_empty() {
        return Err(InternalApiError::bad_request("run event id is required"));
    }
    let event = state
        .run_service
        .list_run_events(run.id.as_str())
        .await
        .map_err(InternalApiError::internal)?
        .into_iter()
        .find(|event| event.id == event_id && event.run_id == run.id)
        .ok_or_else(|| InternalApiError::not_found("run event not found for message"))?;
    let tool_text_limit_chars = state
        .task_service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(InternalApiError::internal)?
        .per_result_max_chars;
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageTaskRunEvent::from(trim_event_for_chatos_detail(event, tool_text_limit_chars)),
    )?))
}

async fn require_chatos_message_run(
    state: &AppState,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<TaskRunRecord, InternalApiError> {
    let run = state
        .run_service
        .get_run(run_id)
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    state
        .task_service
        .get_message_task_detail_for_chatos_source(
            run.task_id.as_str(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    Ok(run)
}

async fn get_chatos_message_graph_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ChatosMessageRunQuery>,
) -> Result<Json<Value>, InternalApiError> {
    require_chatos_internal_auth(&state, &headers)?;
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query.source)?;
    let (event_limit, event_offset) = run_event_page(&query);
    let run = state
        .run_service
        .get_run(run_id.trim())
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for graph"))?;
    let graph = state
        .task_service
        .get_message_task_graph_for_chatos_source(
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?;
    let task = graph
        .nodes
        .into_iter()
        .find(|node| node.task.id == run.task_id)
        .map(|node| node.task)
        .ok_or_else(|| InternalApiError::not_found("run not found for graph"))?;
    let events = state
        .run_service
        .list_run_events(run.id.as_str())
        .await
        .map_err(InternalApiError::internal)?;
    let tool_text_limit_chars = state
        .task_service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(InternalApiError::internal)?
        .per_result_max_chars;
    let (events, events_total, events_has_more) =
        paginate_run_events(events, event_limit, event_offset, tool_text_limit_chars);
    let model_config = state
        .model_config_service
        .get_model_config(run.model_config_id.as_str())
        .await
        .map_err(InternalApiError::internal)?
        .map(ChatosMessageModelConfigSummary::from);
    Ok(Json(redact_workspace_paths_internal(
        &state,
        ChatosMessageRunDetail {
            task,
            run: ChatosMessageTaskRun::from(trim_run_for_chatos_detail(run)),
            model_config,
            events,
            events_total,
            events_limit: event_limit,
            events_offset: event_offset,
            events_has_more,
        },
    )?))
}

fn require_chatos_internal_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TaskRunnerInternalRequestIdentity, InternalApiError> {
    require_task_runner_internal_request(
        &state.config,
        headers,
        &[CHATOS_CALLER],
        CHATOS_MESSAGES_READ_SCOPE,
    )
    .map_err(|err| InternalApiError {
        status: err.status,
        message: err.message,
    })
}

#[cfg(test)]
#[path = "chatos_internal/tests.rs"]
mod tests;
