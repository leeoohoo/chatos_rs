// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{
    is_task_runner_execution_agent as is_task_runner_execution_key,
    is_task_runner_planning_agent as is_task_runner_planning_key, parse_system_agent_key,
};

use super::*;

pub(super) fn mcp_management_binding_from_headers(
    headers: &HeaderMap,
) -> Result<McpManagementBinding, String> {
    let required =
        |key: &'static str| header_text(headers, key).ok_or_else(|| format!("{key} is required"));
    let owner_user_id = required("x-mcp-management-owner-user-id")?;
    let agent_key_text = required("x-mcp-management-agent-key")?;
    let agent_key = parse_system_agent_key(&agent_key_text)
        .ok_or_else(|| "x-mcp-management-agent-key is not a registered System Agent".to_string())?;
    Ok(McpManagementBinding {
        owner_user_id,
        owner_role: header_text(headers, "x-mcp-management-owner-role"),
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
        contact_agent_id: header_text(headers, "x-mcp-management-contact-agent-id"),
        default_model_config_id: header_text(headers, "x-mcp-management-default-model-config-id"),
        task_profile: header_text(headers, "x-mcp-management-task-profile")
            .map(|value| crate::models::normalize_task_profile(Some(value.as_str())))
            .transpose()?,
        expected_project_task_ids: header_csv_set(
            headers,
            "x-mcp-management-expected-project-task-ids",
        ),
    })
}

pub(super) fn task_matches_mcp_management_binding(
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

pub(super) fn task_matches_bound_agent(
    task: &crate::models::TaskRecord,
    agent_key: chatos_plugin_management_sdk::SystemAgentKey,
) -> bool {
    let planning = crate::models::uses_task_runner_planning_agent(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    );
    if planning {
        is_task_runner_planning_key(agent_key)
    } else {
        is_task_runner_execution_key(agent_key)
    }
}

pub(super) fn bound_ask_user_prompt_timeout_ms(
    binding: &McpManagementBinding,
) -> Result<u64, String> {
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

pub(super) fn task_runner_mcp_text_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "_structured_result": payload,
    })
}

pub(super) fn task_runner_mcp_error(
    id: Value,
    code: i32,
    message: impl Into<String>,
) -> JsonRpcResponse {
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

pub(super) async fn downstream_access_token_from_headers(
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

pub(super) fn user_access_token_from_headers(
    headers: &HeaderMap,
) -> Result<Option<String>, ApiError> {
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

pub(super) fn ensure_same_owner_scope(
    agent_user: &CurrentUser,
    user: &CurrentUser,
) -> Result<(), ApiError> {
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

pub(super) fn mcp_request_context_from_headers(
    headers: &HeaderMap,
) -> Result<McpRequestContext, String> {
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
pub(super) enum HeaderSelectedPlugin {
    Id(String),
    Ref(chatos_plugin_management_sdk::SelectedPluginRef),
}

pub(super) fn plugin_config_override_from_headers(
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

pub(super) fn decode_header_plugin_command_invocations(
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

pub(super) fn normalize_header_plugin_command_invocations(
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

pub(super) fn normalize_header_selected_plugins(
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

pub(super) fn normalize_header_ids(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter_map(|value| {
            let value = value.trim().to_string();
            (!value.is_empty() && seen.insert(value.clone())).then_some(value)
        })
        .collect()
}

pub(super) fn header_csv_set(
    headers: &HeaderMap,
    key: &'static str,
) -> std::collections::BTreeSet<String> {
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

pub(super) fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn header_bool(headers: &HeaderMap, key: &'static str) -> bool {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}
