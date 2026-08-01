// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::core::{bearer_token_from_headers, current_user_from_user_service_token};
use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chatos_mcp::{AskUserOptions, AskUserService, AskUserStoreRef};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

use super::internal_auth::{
    require_task_runner_internal_request, MCP_MANAGEMENT_CALLER, MCP_TOOLS_CALL_SCOPE,
    MCP_TOOLS_LIST_SCOPE,
};

const PLUGIN_CONNECTOR_RESPONSE_LIMIT_BYTES: usize = 1024 * 1024;
const PLUGIN_SELECTION_HEADER_LIMIT_BYTES: usize = 16 * 1024;
const PLUGIN_SELECTION_MAX_ITEMS: usize = 50;
const PLUGIN_COMMAND_INVOCATION_HEADER_JSON_LIMIT_BYTES: usize = 256 * 1024;
const PLUGIN_COMMAND_INVOCATION_HEADER_ENCODED_LIMIT_BYTES: usize =
    PLUGIN_COMMAND_INVOCATION_HEADER_JSON_LIMIT_BYTES.div_ceil(3) * 4;
const PLUGIN_COMMAND_INVOCATION_MAX_ITEMS: usize = 64;
const PLUGIN_COMMAND_ARGUMENT_LIMIT_BYTES: usize = 16 * 1024;
const ASK_USER_SESSION_EXPIRY_SAFETY_MARGIN_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct PluginConnectorDeviceView {
    id: String,
    display_name: String,
    client_version: Option<String>,
    os: Option<String>,
    status: String,
    last_seen_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct PluginConnectorWorkspaceView {
    id: String,
    device_id: String,
    display_name: String,
    local_path_alias: String,
    capabilities: Vec<String>,
    status: String,
}

pub(super) async fn list_mcp_catalog(State(state): State<AppState>) -> Json<Vec<McpCatalogEntry>> {
    Json(state.mcp_catalog_service.list_catalog())
}

pub(super) async fn list_task_capability_catalog(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<TaskCapabilityCatalogQuery>,
) -> Result<Json<Value>, ApiError> {
    let owner_user_id = user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("current user is missing owner scope"))?;
    let task_profile = crate::models::normalize_task_profile(query.task_profile.as_deref())
        .map_err(ApiError::bad_request)?;
    let agent_key = crate::models::task_runner_agent_key_for_runtime(
        task_profile.as_str(),
        query.requires_execution.unwrap_or(true),
        query.runtime_provider.as_deref(),
    );
    let policy = state
        .task_service
        .resolve_task_runner_policy_for_agent_runtime(
            Some(&user),
            Some(owner_user_id),
            agent_key,
            query.device_id.clone(),
            query.runtime_provider.clone(),
        )
        .await
        .map_err(ApiError::bad_gateway)?
        .ok_or_else(|| ApiError::internal("plugin management policy resolver is unavailable"))?;
    Ok(Json(json!({
        "agent_key": agent_key.as_str(),
        "policy_revision": policy.policy_revision(),
        "selectable_plugins": policy.selectable_plugin_views(),
    })))
}

pub(super) async fn list_plugin_connectors() -> Result<Json<Value>, ApiError> {
    let access_token = crate::auth::get_current_access_token()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("current user access token is unavailable"))?;
    let base_url = crate::services::plugin_relay_base_url().map_err(ApiError::bad_gateway)?;
    let timeout_ms = std::env::var("TASK_RUNNER_PLUGIN_CONNECTOR_DISCOVERY_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10_000)
        .clamp(1_000, 30_000);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            ApiError::internal(format!("build Local Connector client failed: {error}"))
        })?;
    let devices_url = format!("{base_url}/api/local-connectors/devices");
    let workspaces_url = format!("{base_url}/api/local-connectors/workspaces");
    let (devices, workspaces) = tokio::try_join!(
        fetch_plugin_connector_json::<Vec<PluginConnectorDeviceView>>(
            &client,
            devices_url.as_str(),
            access_token.as_str(),
        ),
        fetch_plugin_connector_json::<Vec<PluginConnectorWorkspaceView>>(
            &client,
            workspaces_url.as_str(),
            access_token.as_str(),
        )
    )?;
    Ok(Json(json!({
        "devices": devices,
        "workspaces": workspaces,
    })))
}

async fn fetch_plugin_connector_json<T>(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    let response = client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| ApiError {
            status: upstream_gateway_status(&error),
            message: format!("Local Connector discovery request failed: {error}"),
        })?;
    let status = response.status();
    let bytes = read_response_bytes_limited(response, PLUGIN_CONNECTOR_RESPONSE_LIMIT_BYTES)
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!(
                "read Local Connector discovery response failed: {error}"
            ))
        })?;
    if !status.is_success() {
        let message = serde_json::from_slice::<Value>(bytes.as_slice())
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Local Connector discovery was rejected".to_string());
        return Err(ApiError::bad_gateway(format!(
            "Local Connector discovery failed with {status}: {message}"
        )));
    }
    serde_json::from_slice(bytes.as_slice()).map_err(|error| {
        ApiError::bad_gateway(format!(
            "decode Local Connector discovery response failed: {error}"
        ))
    })
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskCapabilityCatalogQuery {
    task_profile: Option<String>,
    requires_execution: Option<bool>,
    runtime_provider: Option<String>,
    device_id: Option<String>,
}

pub(super) async fn get_mcp_server_info(State(state): State<AppState>) -> Json<McpServerInfo> {
    Json(state.task_runner_mcp_service.server_info())
}

pub(super) async fn get_mcp_provider_descriptor(
    State(state): State<AppState>,
) -> Json<crate::models::McpProviderDescriptor> {
    Json(state.task_runner_mcp_service.provider_descriptor())
}

pub(super) async fn preview_mcp_prompt(
    State(state): State<AppState>,
    Json(input): Json<McpPromptPreviewRequest>,
) -> Result<Json<McpPromptPreviewResponse>, ApiError> {
    let preview = state
        .mcp_catalog_service
        .preview_prompt(input)
        .map_err(ApiError::bad_request)?;
    Ok(Json(redact_workspace_paths(&state, preview)?))
}

pub(super) async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let request_method = request.method.clone();
    let request_tool_name = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp request received"
    );
    let agent_access_token = match bearer_token_from_headers(&headers) {
        Ok(token) => token.to_string(),
        Err(err) => {
            let err = ApiError::unauthorized(err);
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
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp agent token extracted"
    );
    let current_user =
        match current_user_from_user_service_token(&state.config, &agent_access_token).await {
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
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp agent token verified"
    );
    let downstream_access_token = match downstream_access_token_from_headers(
        &state.config,
        &headers,
        &agent_access_token,
        &current_user,
    )
    .await
    {
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
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp downstream token resolved"
    );
    let request_context = match mcp_request_context_from_headers(&headers) {
        Ok(value) => value,
        Err(message) => {
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(crate::mcp_server::JsonRpcError {
                    code: -32602,
                    message,
                }),
            });
        }
    };
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        project_id = request_context.project_id.as_deref().unwrap_or(""),
        task_profile = request_context.task_profile.as_deref().unwrap_or(""),
        tool_profile = request_context.tool_profile.as_deref().unwrap_or(""),
        "task runner mcp dispatching jsonrpc"
    );
    Json(
        crate::auth::with_access_token_scope(Some(downstream_access_token), async move {
            state
                .task_runner_mcp_service
                .handle_jsonrpc(request, current_user, request_context)
                .await
        })
        .await,
    )
}

#[derive(Debug, Clone)]
struct McpManagementBinding {
    owner_user_id: String,
    agent_key: chatos_plugin_management_sdk::SystemAgentKey,
    session_id: String,
    session_expires_at_unix: i64,
    project_id: String,
    run_id: Option<String>,
    turn_id: Option<String>,
    task_id: Option<String>,
    source_session_id: Option<String>,
    source_user_message_id: Option<String>,
    default_model_config_id: Option<String>,
    expected_project_task_ids: std::collections::BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundTaskProcessLogArgs {
    #[serde(default)]
    operation: crate::models::TaskProcessLogOperation,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    heading: Option<String>,
}

pub(super) async fn mcp_management_entrypoint(
    Path(system_key): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let required_scope = if request.method == chatos_mcp_service::METHOD_TOOLS_LIST {
        MCP_TOOLS_LIST_SCOPE
    } else {
        MCP_TOOLS_CALL_SCOPE
    };
    if let Err(error) = require_task_runner_internal_request(
        &state.config,
        &headers,
        &[MCP_MANAGEMENT_CALLER],
        required_scope,
    ) {
        return Json(task_runner_mcp_error(id, -32001, error.message));
    }
    let binding = match mcp_management_binding_from_headers(&headers) {
        Ok(binding) => binding,
        Err(message) => return Json(task_runner_mcp_error(id, -32602, message)),
    };
    let system_key = match system_key.parse::<chatos_plugin_management_sdk::SystemMcpKey>() {
        Ok(system_key) => system_key,
        Err(message) => return Json(task_runner_mcp_error(id, -32602, message)),
    };
    if request.method == chatos_mcp_service::METHOD_TOOLS_LIST {
        return Json(match system_key {
            chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService => {
                dispatch_bound_task_runner_tool(&state, request, &binding).await
            }
            _ => task_runner_mcp_error(
                id,
                -32601,
                "Task Runner internal MCP Provider only exposes dynamic tools/list for Task Runner Service MCP",
            ),
        });
    }
    if request.method != chatos_mcp_service::METHOD_TOOLS_CALL {
        return Json(task_runner_mcp_error(
            id,
            -32601,
            "Task Runner internal MCP Provider only accepts tools/call",
        ));
    }
    let response = match system_key {
        chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService => {
            dispatch_bound_task_runner_tool(&state, request, &binding).await
        }
        chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog => {
            dispatch_bound_task_process_log(&state, request, &binding).await
        }
        chatos_plugin_management_sdk::SystemMcpKey::AskUser => {
            dispatch_bound_ask_user(&state, request, &binding).await
        }
        _ => task_runner_mcp_error(
            id,
            -32602,
            "Task Runner internal MCP Provider does not own this System MCP",
        ),
    };
    Json(response)
}

async fn dispatch_bound_task_runner_tool(
    state: &AppState,
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    use chatos_plugin_management_sdk::SystemAgentKey;

    if !matches!(
        binding.agent_key,
        SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
    ) {
        return task_runner_mcp_error(
            request.id.unwrap_or(Value::Null),
            -32001,
            "configured Agent is not allowed to use Task Runner Service MCP",
        );
    }
    let current_user = CurrentUser {
        id: format!("mcp-management:{}", binding.session_id),
        username: format!("mcp-management-{}", binding.agent_key.as_str()),
        display_name: format!("MCP Management {}", binding.agent_key.as_str()),
        role: crate::models::UserRole::Agent,
        owner_user_id: Some(binding.owner_user_id.clone()),
        owner_username: None,
        owner_display_name: None,
    };
    let is_requirement_planner =
        binding.agent_key == SystemAgentKey::ProjectRequirementExecutionPlannerAgent;
    let is_chatos_plan = binding.agent_key == SystemAgentKey::ChatosPlanningAgent;
    let request_context = McpRequestContext {
        project_id: Some(binding.project_id.clone()),
        source_session_id: binding.source_session_id.clone(),
        source_turn_id: binding.turn_id.clone(),
        source_user_message_id: binding.source_user_message_id.clone(),
        default_model_config_id: binding.default_model_config_id.clone(),
        workspace_dir: None,
        remote_server_config: None,
        tool_profile: Some(
            if is_requirement_planner {
                "project_requirement_execution_planner"
            } else {
                "chatos_async_planner"
            }
            .to_string(),
        ),
        task_profile: is_chatos_plan.then(|| crate::models::TASK_PROFILE_CHATOS_PLAN.to_string()),
        builtin_prompt_locale: None,
        chatos_plan_mode: is_chatos_plan,
        expected_project_task_ids: binding.expected_project_task_ids.clone(),
        plugin_config_override: None,
    };
    state
        .task_runner_mcp_service
        .handle_jsonrpc(request, current_user, request_context)
        .await
}

async fn dispatch_bound_task_process_log(
    state: &AppState,
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    use chatos_plugin_management_sdk::SystemAgentKey;

    let id = request.id.unwrap_or(Value::Null);
    if !matches!(
        binding.agent_key,
        SystemAgentKey::TaskRunnerPlanPhase | SystemAgentKey::TaskRunnerRunPhase
    ) {
        return task_runner_mcp_error(
            id,
            -32001,
            "configured Agent is not allowed to use Task Process Log MCP",
        );
    }
    let Some(task_id) = binding.task_id.as_deref() else {
        return task_runner_mcp_error(id, -32602, "Task Process Log requires bound task_id");
    };
    let Some(run_id) = binding.run_id.as_deref() else {
        return task_runner_mcp_error(id, -32602, "Task Process Log requires bound run_id");
    };
    let name = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim);
    if name != Some("record_process") {
        return task_runner_mcp_error(id, -32602, "Task Process Log tool was not found");
    }
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let input = match serde_json::from_value::<BoundTaskProcessLogArgs>(arguments) {
        Ok(input) => input,
        Err(error) => {
            return task_runner_mcp_error(
                id,
                -32602,
                format!("invalid Task Process Log arguments: {error}"),
            )
        }
    };
    let run = match state.run_service.get_run(run_id).await {
        Ok(Some(run)) => run,
        Ok(None) => {
            return task_runner_mcp_error(id, -32000, "bound Task Runner run was not found")
        }
        Err(error) => return task_runner_mcp_error(id, -32000, error),
    };
    if run.task_id != task_id {
        return task_runner_mcp_error(id, -32001, "bound run does not belong to bound task");
    }
    if run.status != crate::models::TaskRunStatus::Running {
        return task_runner_mcp_error(id, -32001, "bound Task Runner run is no longer active");
    }
    let task = match state.task_service.get_task(task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return task_runner_mcp_error(id, -32000, "bound Task Runner task was not found")
        }
        Err(error) => return task_runner_mcp_error(id, -32000, error),
    };
    if !task_matches_mcp_management_binding(&task, binding)
        || !task_matches_bound_agent(&task, binding.agent_key)
    {
        return task_runner_mcp_error(
            id,
            -32001,
            "bound task does not match MCP Management owner, project, run, or Agent scope",
        );
    }
    let updated = match state
        .task_service
        .record_task_process(
            task_id,
            crate::models::RecordTaskProcessRequest {
                operation: input.operation,
                content: input.content,
                heading: input.heading,
            },
        )
        .await
    {
        Ok(Some(task)) => task,
        Ok(None) => {
            return task_runner_mcp_error(id, -32000, "bound Task Runner task was not found")
        }
        Err(error) => return task_runner_mcp_error(id, -32000, error),
    };
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(task_runner_mcp_text_result(json!({
            "recorded": true,
            "task_id": updated.id,
            "run_id": run_id,
            "process_log_chars": updated
                .process_log
                .as_deref()
                .map(|value| value.chars().count())
                .unwrap_or_default(),
            "updated_at": updated.updated_at,
        }))),
        error: None,
    }
}

async fn dispatch_bound_ask_user(
    state: &AppState,
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    use chatos_plugin_management_sdk::SystemAgentKey;

    let id = request.id.unwrap_or(Value::Null);
    if !matches!(
        binding.agent_key,
        SystemAgentKey::TaskRunnerPlanPhase | SystemAgentKey::TaskRunnerRunPhase
    ) {
        return task_runner_mcp_error(
            id,
            -32001,
            "configured Agent is not allowed to use Task Runner Ask User MCP",
        );
    }
    let Some(task_id) = binding.task_id.as_deref() else {
        return task_runner_mcp_error(id, -32602, "Ask User requires bound task_id");
    };
    let Some(run_id) = binding.run_id.as_deref() else {
        return task_runner_mcp_error(id, -32602, "Ask User requires bound run_id");
    };
    let run = match state.run_service.get_run(run_id).await {
        Ok(Some(run)) => run,
        Ok(None) => {
            return task_runner_mcp_error(id, -32000, "bound Task Runner run was not found")
        }
        Err(error) => return task_runner_mcp_error(id, -32000, error),
    };
    if run.task_id != task_id {
        return task_runner_mcp_error(id, -32001, "bound run does not belong to bound task");
    }
    if run.status != crate::models::TaskRunStatus::Running {
        return task_runner_mcp_error(id, -32001, "bound Task Runner run is no longer active");
    }
    let task = match state.task_service.get_task(task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            return task_runner_mcp_error(id, -32000, "bound Task Runner task was not found")
        }
        Err(error) => return task_runner_mcp_error(id, -32000, error),
    };
    if !task_matches_mcp_management_binding(&task, binding)
        || !task_matches_bound_agent(&task, binding.agent_key)
    {
        return task_runner_mcp_error(
            id,
            -32001,
            "bound task does not match MCP Management owner, project, run, or Agent scope",
        );
    }
    let prompt_timeout_ms = match bound_ask_user_prompt_timeout_ms(binding) {
        Ok(timeout_ms) => timeout_ms,
        Err(message) => return task_runner_mcp_error(id, -32001, message),
    };
    let service = match AskUserService::new(AskUserOptions {
        server_name: chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::AskUser,
        )
        .server_name
        .to_string(),
        prompt_timeout_ms,
        store: AskUserStoreRef::new(Arc::new(state.ask_user_prompt_service.clone())),
    }) {
        Ok(service) => service,
        Err(error) => return task_runner_mcp_error(id, -32000, error),
    };
    let Some(name) = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return task_runner_mcp_error(id, -32602, "Ask User tool name is required");
    };
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match service.call_tool(name, arguments, Some(task_id), Some(run_id), None) {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        },
        Err(error) => task_runner_mcp_error(id, -32000, error),
    }
}

fn mcp_management_binding_from_headers(
    headers: &HeaderMap,
) -> Result<McpManagementBinding, String> {
    let required =
        |key: &'static str| header_text(headers, key).ok_or_else(|| format!("{key} is required"));
    let owner_user_id = required("x-mcp-management-owner-user-id")?;
    let agent_key_text = required("x-mcp-management-agent-key")?;
    let agent_key = chatos_plugin_management_sdk::SystemAgentKey::ALL
        .into_iter()
        .find(|key| key.as_str() == agent_key_text)
        .ok_or_else(|| "x-mcp-management-agent-key is not a registered System Agent".to_string())?;
    Ok(McpManagementBinding {
        owner_user_id,
        agent_key,
        session_id: required("x-mcp-management-session-id")?,
        session_expires_at_unix: required("x-mcp-management-session-expires-at-unix")?
            .parse::<i64>()
            .map_err(|_| {
                "x-mcp-management-session-expires-at-unix must be an integer".to_string()
            })?,
        project_id: required("x-mcp-management-project-id")?,
        run_id: header_text(headers, "x-mcp-management-run-id"),
        turn_id: header_text(headers, "x-mcp-management-turn-id"),
        task_id: header_text(headers, "x-mcp-management-task-id"),
        source_session_id: header_text(headers, "x-mcp-management-source-session-id"),
        source_user_message_id: header_text(headers, "x-mcp-management-source-user-message-id"),
        default_model_config_id: header_text(headers, "x-mcp-management-default-model-config-id"),
        expected_project_task_ids: header_csv_set(
            headers,
            "x-mcp-management-expected-project-task-ids",
        ),
    })
}

fn task_matches_mcp_management_binding(
    task: &crate::models::TaskRecord,
    binding: &McpManagementBinding,
) -> bool {
    let owner_user_id = task
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            task.creator_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        });
    owner_user_id == Some(binding.owner_user_id.as_str())
        && task.project_id.trim() == binding.project_id
        && binding.run_id.as_deref().is_some_and(|run_id| {
            task.last_run_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|last_run_id| last_run_id == run_id)
        })
}

fn task_matches_bound_agent(
    task: &crate::models::TaskRecord,
    agent_key: chatos_plugin_management_sdk::SystemAgentKey,
) -> bool {
    use chatos_plugin_management_sdk::SystemAgentKey;

    let planning = crate::models::uses_task_runner_planning_agent(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    );
    if planning {
        agent_key == SystemAgentKey::TaskRunnerPlanPhase
    } else {
        agent_key == SystemAgentKey::TaskRunnerRunPhase
    }
}

fn bound_ask_user_prompt_timeout_ms(binding: &McpManagementBinding) -> Result<u64, String> {
    let now_unix = chrono::Utc::now().timestamp();
    let remaining_seconds = binding.session_expires_at_unix.saturating_sub(now_unix);
    let remaining_ms = u64::try_from(remaining_seconds)
        .unwrap_or_default()
        .saturating_mul(1_000);
    let usable_ms = remaining_ms.saturating_sub(ASK_USER_SESSION_EXPIRY_SAFETY_MARGIN_MS);
    if usable_ms < 10_000 {
        return Err("MCP Management session expires too soon to start Ask User".to_string());
    }
    Ok(usable_ms.min(chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT))
}

fn task_runner_mcp_text_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "_structured_result": payload,
    })
}

fn task_runner_mcp_error(id: Value, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(crate::mcp_server::JsonRpcError {
            code,
            message: message.into(),
        }),
    }
}

async fn downstream_access_token_from_headers(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
    agent_access_token: &str,
    agent_user: &CurrentUser,
) -> Result<String, ApiError> {
    let Some(user_access_token) = user_access_token_from_headers(headers)? else {
        return Ok(agent_access_token.to_string());
    };
    let user = current_user_from_user_service_token(config, user_access_token.as_str()).await?;
    ensure_same_owner_scope(agent_user, &user)?;
    Ok(user_access_token)
}

fn user_access_token_from_headers(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    for key in [
        "x-chatos-user-authorization",
        "x-user-service-authorization",
        "x-chatos-user-token",
    ] {
        let Some(value) = header_text(headers, key) else {
            continue;
        };
        let token = if let Some(token) = value.strip_prefix("Bearer ").map(str::trim) {
            token
        } else if let Some(token) = value.strip_prefix("bearer ").map(str::trim) {
            token
        } else {
            value.as_str()
        };
        if token.is_empty() {
            continue;
        }
        return Ok(Some(token.to_string()));
    }
    Ok(None)
}

fn ensure_same_owner_scope(agent_user: &CurrentUser, user: &CurrentUser) -> Result<(), ApiError> {
    let agent_owner = agent_user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("agent token missing owner scope"))?;
    let user_owner = user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("user token missing owner scope"))?;
    if agent_owner == user_owner {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "agent token and user token owner scope do not match",
        ))
    }
}

fn mcp_request_context_from_headers(headers: &HeaderMap) -> Result<McpRequestContext, String> {
    Ok(McpRequestContext {
        project_id: header_text(headers, "x-chatos-project-id")
            .or_else(|| header_text(headers, "x-task-runner-project-id")),
        source_session_id: header_text(headers, "x-chatos-session-id")
            .or_else(|| header_text(headers, "x-chatos-conversation-id")),
        source_turn_id: header_text(headers, "x-chatos-turn-id"),
        source_user_message_id: header_text(headers, "x-chatos-user-message-id"),
        default_model_config_id: header_text(headers, "x-task-runner-default-model-config-id"),
        workspace_dir: header_text(headers, "x-task-runner-workspace-dir")
            .or_else(|| header_text(headers, "x-chatos-workspace-dir"))
            .or_else(|| header_text(headers, "x-chatos-workspace-root")),
        remote_server_config: header_text(headers, "x-task-runner-remote-server-config")
            .or_else(|| header_text(headers, "x-task-runner-remote-server-json")),
        tool_profile: header_text(headers, "x-task-runner-tool-profile"),
        task_profile: header_text(headers, "x-task-runner-task-profile"),
        builtin_prompt_locale: header_text(headers, "x-task-runner-builtin-prompt-locale")
            .or_else(|| header_text(headers, "x-chatos-internal-context-locale")),
        chatos_plan_mode: header_bool(headers, "x-chatos-plan-mode"),
        expected_project_task_ids: header_csv_set(
            headers,
            "x-task-runner-expected-project-task-ids",
        ),
        plugin_config_override: plugin_config_override_from_headers(headers)?,
    })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HeaderSelectedPlugin {
    Id(String),
    Ref(chatos_plugin_management_sdk::SelectedPluginRef),
}

fn plugin_config_override_from_headers(
    headers: &HeaderMap,
) -> Result<Option<chatos_plugin_management_sdk::TaskPluginConfig>, String> {
    let device_id = header_text(headers, "x-task-runner-plugin-device-id");
    let workspace_id = header_text(headers, "x-task-runner-plugin-workspace-id");
    let selected_plugins_header = header_text(headers, "x-task-runner-selected-plugins");
    let command_invocations_header =
        header_text(headers, "x-task-runner-plugin-command-invocations");
    if device_id.is_none()
        && workspace_id.is_none()
        && selected_plugins_header.is_none()
        && command_invocations_header.is_none()
    {
        return Ok(None);
    }

    let selected_plugins = match selected_plugins_header {
        Some(value) => {
            if value.len() > PLUGIN_SELECTION_HEADER_LIMIT_BYTES {
                return Err(format!(
                    "x-task-runner-selected-plugins exceeds {PLUGIN_SELECTION_HEADER_LIMIT_BYTES} bytes"
                ));
            }
            let decoded = serde_json::from_str::<Vec<HeaderSelectedPlugin>>(value.as_str())
                .map_err(|error| format!("invalid x-task-runner-selected-plugins JSON: {error}"))?;
            if decoded.len() > PLUGIN_SELECTION_MAX_ITEMS {
                return Err(format!(
                    "x-task-runner-selected-plugins exceeds {PLUGIN_SELECTION_MAX_ITEMS} items"
                ));
            }
            normalize_header_selected_plugins(decoded)
        }
        None => Vec::new(),
    };
    let command_invocations = match command_invocations_header {
        Some(value) => decode_header_plugin_command_invocations(value.as_str())?,
        None => Vec::new(),
    };

    Ok(Some(chatos_plugin_management_sdk::TaskPluginConfig {
        device_id,
        workspace_id,
        selected_plugins,
        command_invocations,
    }))
}

fn decode_header_plugin_command_invocations(
    value: &str,
) -> Result<Vec<chatos_plugin_management_sdk::PluginCommandInvocation>, String> {
    if value.len() > PLUGIN_COMMAND_INVOCATION_HEADER_ENCODED_LIMIT_BYTES {
        return Err(format!(
            "x-task-runner-plugin-command-invocations exceeds {PLUGIN_COMMAND_INVOCATION_HEADER_ENCODED_LIMIT_BYTES} encoded bytes"
        ));
    }
    let payload = if value.trim_start().starts_with('[') {
        value.as_bytes().to_vec()
    } else {
        URL_SAFE_NO_PAD.decode(value.as_bytes()).map_err(|error| {
            format!("invalid x-task-runner-plugin-command-invocations base64: {error}")
        })?
    };
    if payload.len() > PLUGIN_COMMAND_INVOCATION_HEADER_JSON_LIMIT_BYTES {
        return Err(format!(
            "x-task-runner-plugin-command-invocations exceeds {PLUGIN_COMMAND_INVOCATION_HEADER_JSON_LIMIT_BYTES} decoded bytes"
        ));
    }
    let decoded = serde_json::from_slice::<
        Vec<chatos_plugin_management_sdk::PluginCommandInvocation>,
    >(payload.as_slice())
    .map_err(|error| format!("invalid x-task-runner-plugin-command-invocations JSON: {error}"))?;
    if decoded.len() > PLUGIN_COMMAND_INVOCATION_MAX_ITEMS {
        return Err(format!(
            "x-task-runner-plugin-command-invocations exceeds {PLUGIN_COMMAND_INVOCATION_MAX_ITEMS} items"
        ));
    }
    normalize_header_plugin_command_invocations(decoded)
}

fn normalize_header_plugin_command_invocations(
    values: Vec<chatos_plugin_management_sdk::PluginCommandInvocation>,
) -> Result<Vec<chatos_plugin_management_sdk::PluginCommandInvocation>, String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| {
            let plugin_id = value.plugin_id.trim().to_string();
            let command_id = value.command_id.trim().to_string();
            if plugin_id.is_empty() || command_id.is_empty() {
                return Err(
                    "Plugin Command invocation requires non-empty plugin_id and command_id"
                        .to_string(),
                );
            }
            if plugin_id.contains('\0') || command_id.contains('\0') {
                return Err("Plugin Command invocation identity contains NUL bytes".to_string());
            }
            if !seen.insert((plugin_id.clone(), command_id.clone())) {
                return Err(format!(
                    "Plugin Command invocation is duplicated: {plugin_id}:{command_id}"
                ));
            }
            let arguments = value
                .arguments
                .as_deref()
                .map(str::trim)
                .filter(|arguments| !arguments.is_empty());
            if arguments.is_some_and(|arguments| {
                arguments.contains('\0')
                    || arguments.len() > PLUGIN_COMMAND_ARGUMENT_LIMIT_BYTES
            }) {
                return Err(format!(
                    "Plugin Command arguments exceed {PLUGIN_COMMAND_ARGUMENT_LIMIT_BYTES} bytes or contain NUL"
                ));
            }
            Ok(chatos_plugin_management_sdk::PluginCommandInvocation {
                plugin_id,
                command_id,
                arguments: arguments.map(str::to_string),
            })
        })
        .collect()
}

fn normalize_header_selected_plugins(
    values: Vec<HeaderSelectedPlugin>,
) -> Vec<chatos_plugin_management_sdk::SelectedPluginRef> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter_map(|value| {
            let mut selected = match value {
                HeaderSelectedPlugin::Id(plugin_id) => {
                    chatos_plugin_management_sdk::SelectedPluginRef {
                        plugin_id,
                        selected_skill_ids: Vec::new(),
                        selected_command_ids: Vec::new(),
                        selected_agent_ids: Vec::new(),
                    }
                }
                HeaderSelectedPlugin::Ref(value) => value,
            };
            selected.plugin_id = selected.plugin_id.trim().to_string();
            if selected.plugin_id.is_empty() || !seen.insert(selected.plugin_id.clone()) {
                return None;
            }
            selected.selected_skill_ids = normalize_header_ids(selected.selected_skill_ids);
            selected.selected_command_ids = normalize_header_ids(selected.selected_command_ids);
            selected.selected_agent_ids = normalize_header_ids(selected.selected_agent_ids);
            Some(selected)
        })
        .collect()
}

fn normalize_header_ids(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter_map(|value| {
            let value = value.trim().to_string();
            (!value.is_empty() && seen.insert(value.clone())).then_some(value)
        })
        .collect()
}

fn header_csv_set(headers: &HeaderMap, key: &'static str) -> std::collections::BTreeSet<String> {
    header_text(headers, key)
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn header_bool(headers: &HeaderMap, key: &'static str) -> bool {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_management_binding_requires_registered_agent_and_complete_identity() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-mcp-management-owner-user-id",
            " user-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-agent-key",
            " task_runner_run_phase ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-session-id",
            " session-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-session-expires-at-unix",
            " 4102444800 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-project-id",
            " project-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-run-id",
            " run-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-task-id",
            " task-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-source-session-id",
            " source-session-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-source-user-message-id",
            " message-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-mcp-management-expected-project-task-ids",
            " project-task-b,project-task-a,project-task-a "
                .parse()
                .expect("valid header"),
        );

        let binding = mcp_management_binding_from_headers(&headers).expect("valid binding");
        assert_eq!(binding.owner_user_id, "user-1");
        assert_eq!(binding.agent_key.as_str(), "task_runner_run_phase");
        assert_eq!(binding.session_id, "session-1");
        assert_eq!(binding.session_expires_at_unix, 4_102_444_800);
        assert_eq!(binding.project_id, "project-1");
        assert_eq!(binding.run_id.as_deref(), Some("run-1"));
        assert_eq!(binding.task_id.as_deref(), Some("task-1"));
        assert_eq!(
            binding.source_session_id.as_deref(),
            Some("source-session-1")
        );
        assert_eq!(binding.source_user_message_id.as_deref(), Some("message-1"));
        assert_eq!(
            binding.expected_project_task_ids,
            std::collections::BTreeSet::from([
                "project-task-a".to_string(),
                "project-task-b".to_string(),
            ])
        );

        headers.insert(
            "x-mcp-management-agent-key",
            "arbitrary-agent".parse().expect("valid header"),
        );
        assert!(mcp_management_binding_from_headers(&headers)
            .expect_err("unknown agent must fail")
            .contains("registered System Agent"));
    }

    #[test]
    fn ask_user_timeout_stays_inside_the_immutable_session_lifetime() {
        let binding = McpManagementBinding {
            owner_user_id: "user-1".to_string(),
            agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase,
            session_id: "session-1".to_string(),
            session_expires_at_unix: chrono::Utc::now().timestamp() + 30 * 60,
            project_id: "project-1".to_string(),
            run_id: Some("run-1".to_string()),
            turn_id: None,
            task_id: Some("task-1".to_string()),
            source_session_id: None,
            source_user_message_id: None,
            default_model_config_id: None,
            expected_project_task_ids: std::collections::BTreeSet::new(),
        };

        let timeout = bound_ask_user_prompt_timeout_ms(&binding).expect("usable session lifetime");
        assert!(timeout <= 25 * 60 * 1_000);
        assert!(timeout >= 24 * 60 * 1_000);

        let mut expiring = binding;
        expiring.session_expires_at_unix = chrono::Utc::now().timestamp() + 60;
        assert!(bound_ask_user_prompt_timeout_ms(&expiring).is_err());
    }

    #[test]
    fn task_process_log_arguments_cannot_override_bound_identity() {
        let error = serde_json::from_value::<BoundTaskProcessLogArgs>(json!({
            "operation": "append",
            "content": "verified",
            "heading": null,
            "task_id": "another-task",
            "run_id": "another-run",
            "owner_user_id": "another-user"
        }))
        .expect_err("identity override fields must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn request_context_reads_inherited_model_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-default-model-config-id",
            " model-selected ".parse().expect("valid header"),
        );

        let context = mcp_request_context_from_headers(&headers).expect("valid context");

        assert_eq!(
            context.default_model_config_id.as_deref(),
            Some("model-selected")
        );
    }

    #[test]
    fn request_context_reads_exact_project_task_scope_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-expected-project-task-ids",
            " task-b,task-a,task-a ".parse().expect("valid header"),
        );

        let context = mcp_request_context_from_headers(&headers).expect("valid context");

        assert_eq!(
            context.expected_project_task_ids,
            std::collections::BTreeSet::from(["task-a".to_string(), "task-b".to_string()])
        );
    }

    #[test]
    fn request_context_reads_and_normalizes_user_plugin_selection() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-plugin-device-id",
            " device-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-task-runner-plugin-workspace-id",
            " workspace-1 ".parse().expect("valid header"),
        );
        headers.insert(
            "x-task-runner-selected-plugins",
            r#"["plugin-a",{"plugin_id":" plugin-a ","selected_skill_ids":[],"selected_command_ids":[]},{"plugin_id":"plugin-b","selected_skill_ids":[" skill-1 ","skill-1"],"selected_command_ids":[" review ","review"]}]"#
                .parse()
                .expect("valid header"),
        );
        let command_invocations = serde_json::to_vec(&vec![
            chatos_plugin_management_sdk::PluginCommandInvocation {
                plugin_id: " plugin-b ".to_string(),
                command_id: " review ".to_string(),
                arguments: Some(" 检查中文参数 ".to_string()),
            },
        ])
        .expect("serialize command invocations");
        headers.insert(
            "x-task-runner-plugin-command-invocations",
            URL_SAFE_NO_PAD
                .encode(command_invocations)
                .parse()
                .expect("valid header"),
        );

        let context = mcp_request_context_from_headers(&headers).expect("valid context");
        let config = context
            .plugin_config_override
            .expect("plugin config override");

        assert_eq!(config.device_id.as_deref(), Some("device-1"));
        assert_eq!(config.workspace_id.as_deref(), Some("workspace-1"));
        assert_eq!(config.selected_plugins.len(), 2);
        assert_eq!(config.selected_plugins[0].plugin_id, "plugin-a");
        assert_eq!(
            config.selected_plugins[1].selected_skill_ids,
            vec!["skill-1".to_string()]
        );
        assert_eq!(
            config.selected_plugins[1].selected_command_ids,
            vec!["review".to_string()]
        );
        assert_eq!(
            config.command_invocations,
            vec![chatos_plugin_management_sdk::PluginCommandInvocation {
                plugin_id: "plugin-b".to_string(),
                command_id: "review".to_string(),
                arguments: Some("检查中文参数".to_string()),
            }]
        );
    }

    #[test]
    fn request_context_rejects_invalid_plugin_selection_json() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-selected-plugins",
            "not-json".parse().expect("valid header"),
        );

        let error = mcp_request_context_from_headers(&headers).expect_err("invalid context");

        assert!(error.contains("invalid x-task-runner-selected-plugins JSON"));
    }

    #[test]
    fn request_context_rejects_duplicate_plugin_command_invocations() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-plugin-command-invocations",
            r#"[{"plugin_id":"plugin-a","command_id":"review","arguments":null},{"plugin_id":" plugin-a ","command_id":" review ","arguments":"src"}]"#
                .parse()
                .expect("valid header"),
        );

        let error = mcp_request_context_from_headers(&headers).expect_err("invalid context");

        assert!(error.contains("Plugin Command invocation is duplicated"));
    }

    #[test]
    fn request_context_rejects_oversized_plugin_command_arguments() {
        let payload = serde_json::to_string(&vec![
            chatos_plugin_management_sdk::PluginCommandInvocation {
                plugin_id: "plugin-a".to_string(),
                command_id: "review".to_string(),
                arguments: Some("a".repeat(PLUGIN_COMMAND_ARGUMENT_LIMIT_BYTES + 1)),
            },
        ])
        .expect("serialize command invocations");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-plugin-command-invocations",
            payload.parse().expect("valid header"),
        );

        let error = mcp_request_context_from_headers(&headers).expect_err("invalid context");

        assert!(error.contains("Plugin Command arguments exceed"));
    }
}
