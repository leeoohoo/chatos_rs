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

mod headers;
use headers::*;

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

pub(super) async fn list_plugin_connectors(
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let access_token = crate::auth::get_current_access_token()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("current user access token is unavailable"))?;
    let base_url =
        crate::services::plugin_relay_base_url(&state.config).map_err(ApiError::bad_gateway)?;
    let client = &state.config.local_connector_http_client;
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
    task_profile: Option<String>,
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
    let is_chatos_plan = binding
        .task_profile
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case(crate::models::TASK_PROFILE_CHATOS_PLAN));
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
        task_profile: binding.task_profile.clone(),
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

#[cfg(test)]
mod tests;
