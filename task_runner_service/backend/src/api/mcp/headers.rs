// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{
    is_task_runner_execution_agent as is_task_runner_execution_key, parse_system_agent_key,
};

use super::*;

const CLIENT_PROJECT_CONTEXT_HEADER: &str = "x-mcp-management-client-project-context";

pub(super) fn mcp_management_binding_from_headers(
    headers: &HeaderMap,
) -> Result<McpManagementBinding, String> {
    let required =
        |key: &'static str| header_text(headers, key).ok_or_else(|| format!("{key} is required"));
    let owner_user_id = required("x-mcp-management-owner-user-id")?;
    let agent_key_text = required("x-mcp-management-agent-key")?;
    let agent_key = parse_system_agent_key(&agent_key_text)
        .ok_or_else(|| "x-mcp-management-agent-key is not a registered System Agent".to_string())?;
    let project_id = header_text(headers, "x-mcp-management-project-id");
    let project_context = client_project_context_from_headers(headers)?;
    match (project_id.as_deref(), project_context.as_ref()) {
        (Some(project_id), Some(snapshot)) => snapshot.validate_project_id(project_id)?,
        (Some(_), None) => {
            return Err(
                "x-mcp-management-client-project-context is required for a project binding"
                    .to_string(),
            )
        }
        (None, Some(_)) => {
            return Err(
                "x-mcp-management-client-project-context requires a project binding".to_string(),
            )
        }
        (None, None) => {}
    }
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
        project_id,
        project_context,
        run_id: header_text(headers, "x-mcp-management-run-id"),
        turn_id: header_text(headers, "x-mcp-management-turn-id"),
        task_id: header_text(headers, "x-mcp-management-task-id"),
        source_session_id: header_text(headers, "x-mcp-management-source-session-id"),
        source_user_message_id: header_text(headers, "x-mcp-management-source-user-message-id"),
        contact_agent_id: header_text(headers, "x-mcp-management-contact-agent-id"),
        default_model_config_id: header_text(headers, "x-mcp-management-default-model-config-id"),
        default_remote_connection_id: header_text(
            headers,
            "x-mcp-management-default-remote-connection-id",
        ),
        task_profile: header_text(headers, "x-mcp-management-task-profile")
            .map(|value| crate::models::normalize_task_profile(Some(value.as_str())))
            .transpose()?,
    })
}

fn client_project_context_from_headers(
    headers: &HeaderMap,
) -> Result<Option<chatos_mcp_management_sdk::ClientProjectContextSnapshot>, String> {
    let Some(encoded) = header_text(headers, CLIENT_PROJECT_CONTEXT_HEADER) else {
        return Ok(None);
    };
    let json = urlencoding::decode(encoded.as_str())
        .map_err(|error| format!("invalid {CLIENT_PROJECT_CONTEXT_HEADER} encoding: {error}"))?;
    let snapshot = serde_json::from_str::<chatos_mcp_management_sdk::ClientProjectContextSnapshot>(
        json.as_ref(),
    )
    .map_err(|error| format!("invalid {CLIENT_PROJECT_CONTEXT_HEADER}: {error}"))?;
    snapshot.validate()?;
    Ok(Some(snapshot))
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
        && task.project_id.as_deref() == binding.project_id.as_deref()
        && binding.run_id.as_deref().is_some_and(|run_id| {
            task.last_run_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|last_run_id| last_run_id == run_id)
        })
}

pub(super) fn task_matches_bound_agent(
    agent_key: chatos_plugin_management_sdk::SystemAgentKey,
) -> bool {
    is_task_runner_execution_key(agent_key)
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
        project_context: None,
        project_id: header_text(headers, "x-chatos-project-id")
            .or_else(|| header_text(headers, "x-task-runner-project-id")),
        source_session_id: header_text(headers, "x-chatos-session-id")
            .or_else(|| header_text(headers, "x-chatos-conversation-id")),
        source_turn_id: header_text(headers, "x-chatos-turn-id"),
        source_user_message_id: header_text(headers, "x-chatos-user-message-id"),
        default_model_config_id: header_text(headers, "x-task-runner-default-model-config-id"),
        remote_connection_id: header_text(headers, "x-chatos-remote-connection-id")
            .or_else(|| header_text(headers, "x-chatos-default-remote-connection-id")),
        workspace_dir: header_text(headers, "x-task-runner-workspace-dir")
            .or_else(|| header_text(headers, "x-chatos-workspace-dir"))
            .or_else(|| header_text(headers, "x-chatos-workspace-root")),
        tool_profile: header_text(headers, "x-task-runner-tool-profile"),
        task_profile: header_text(headers, "x-task-runner-task-profile")
            .map(|value| crate::models::normalize_task_profile(Some(&value)))
            .transpose()?,
        builtin_prompt_locale: header_text(headers, "x-task-runner-builtin-prompt-locale")
            .or_else(|| header_text(headers, "x-chatos-internal-context-locale")),
    })
}

pub(super) fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
