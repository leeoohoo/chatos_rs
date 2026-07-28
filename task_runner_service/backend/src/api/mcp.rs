// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::core::{bearer_token_from_headers, current_user_from_user_service_token};
use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

const PLUGIN_CONNECTOR_RESPONSE_LIMIT_BYTES: usize = 1024 * 1024;
const PLUGIN_SELECTION_HEADER_LIMIT_BYTES: usize = 16 * 1024;
const PLUGIN_SELECTION_MAX_ITEMS: usize = 50;
const PLUGIN_COMMAND_INVOCATION_HEADER_JSON_LIMIT_BYTES: usize = 256 * 1024;
const PLUGIN_COMMAND_INVOCATION_HEADER_ENCODED_LIMIT_BYTES: usize =
    PLUGIN_COMMAND_INVOCATION_HEADER_JSON_LIMIT_BYTES.div_ceil(3) * 4;
const PLUGIN_COMMAND_INVOCATION_MAX_ITEMS: usize = 64;
const PLUGIN_COMMAND_ARGUMENT_LIMIT_BYTES: usize = 16 * 1024;

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
    let agent_key = crate::models::task_runner_agent_key_for(
        task_profile.as_str(),
        query.requires_execution.unwrap_or(true),
    );
    let policy = state
        .task_service
        .resolve_task_runner_policy_for_agent_on_device(
            Some(&user),
            Some(owner_user_id),
            agent_key,
            query.device_id.clone(),
        )
        .await
        .map_err(ApiError::bad_gateway)?
        .ok_or_else(|| ApiError::internal("plugin management policy resolver is unavailable"))?;
    let selectable_builtin_kinds = policy
        .selectable_builtin_kind_names()
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let selectable_builtin_mcps = state
        .mcp_catalog_service
        .list_catalog()
        .into_iter()
        .filter(|item| selectable_builtin_kinds.contains(item.kind.as_str()))
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "agent_key": agent_key.as_str(),
        "policy_revision": policy.policy_revision(),
        "selectable_builtin_mcps": selectable_builtin_mcps,
        "selectable_external_mcps": policy.selectable_external_mcp_views(),
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
