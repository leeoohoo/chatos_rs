// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp::SystemMcpKey;
use chatos_mcp_service::{
    jsonrpc_error, jsonrpc_ok, JsonRpcRequest, JsonRpcResponse, MCP_ERROR_AUTH_REQUIRED,
    MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS, MCP_ERROR_METHOD_NOT_FOUND, METHOD_TOOLS_CALL,
    METHOD_TOOLS_LIST,
};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::api::internal_audit::{
    jsonrpc_outcome, record_chatos_internal_resource_access, ChatosInternalRequestIdentity,
    ChatosInternalResourceAudit,
};
use crate::config::Config;
use crate::modules::conversation_runtime::session_scope::resolve_session_project_scope;
use crate::services::shared_cloud_browser_runtime::CloudBrowserRuntimeBinding;
use crate::services::{chatos_agents, chatos_sessions};

const MCP_MANAGEMENT_CALLER: &str = "mcp-management-service";
const CHATOS_TOKEN_AUDIENCE: &str = "chatos";
const MCP_TOOLS_CALL_SCOPE: &str = "mcp.tools.call";
const CLOUD_BROWSER_SESSION_CLOSE_METHOD: &str = "browser/session/close";
const ASK_USER_SESSION_EXPIRY_SAFETY_MARGIN_MS: u64 = 5 * 60 * 1_000;

mod browser;
mod builtins;
mod validation;
use validation::*;

pub fn router() -> Router {
    Router::new()
        .route(
            "/internal/mcp-management/mcp/{system_key}",
            post(mcp_management_entrypoint),
        )
        .route(
            "/internal/mcp-management/mcp/browser_tools/sessions/{session_id}/close",
            post(browser::close_bound_cloud_browser_session),
        )
}

#[derive(Debug, Clone)]
struct McpManagementBinding {
    owner_user_id: String,
    agent_key: SystemAgentKey,
    session_id: String,
    session_expires_at_unix: i64,
    project_id: String,
    turn_id: Option<String>,
    source_session_id: Option<String>,
    source_user_message_id: Option<String>,
    contact_agent_id: Option<String>,
}

async fn mcp_management_entrypoint(
    Path(system_key): Path<String>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let identity = match require_mcp_management_request(&headers) {
        Ok(identity) => identity,
        Err(message) => return Json(jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, message)),
    };
    let (action, target_name) = mcp_management_audit_action(&request);
    let runtime_session_id = header_text(&headers, "x-mcp-management-session-id")
        .unwrap_or_else(|| identity.trace_id.clone());
    let resource_id = format!("{runtime_session_id}/{system_key}/{target_name}");
    let response = dispatch_mcp_management_request(system_key.as_str(), &headers, request).await;
    record_chatos_internal_resource_access(
        &identity,
        ChatosInternalResourceAudit {
            represented_user_id: header_text(&headers, "x-mcp-management-owner-user-id").as_deref(),
            project_id: header_text(&headers, "x-mcp-management-project-id").as_deref(),
            resource_type: "system_mcp_tool",
            resource_id: resource_id.as_str(),
            resource_name: Some(target_name.as_str()),
            action: action.as_str(),
            outcome: jsonrpc_outcome(&response),
        },
    );
    Json(response)
}

async fn dispatch_mcp_management_request(
    system_key: &str,
    headers: &HeaderMap,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    let binding = match mcp_management_binding_from_headers(&headers) {
        Ok(binding) => binding,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message),
    };
    let system_key = match system_key.parse::<SystemMcpKey>() {
        Ok(system_key) => system_key,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message),
    };
    if system_key == SystemMcpKey::BrowserTools && request.method == METHOD_TOOLS_LIST {
        return browser::dispatch_bound_browser_tools_list(request, &binding).await;
    }
    if request.method != METHOD_TOOLS_CALL {
        return jsonrpc_error(
            id,
            MCP_ERROR_METHOD_NOT_FOUND,
            "ChatOS internal MCP Provider only accepts tools/call, plus tools/list for Browser Tools",
        );
    }
    match system_key {
        SystemMcpKey::AgentBuilder => {
            builtins::dispatch_bound_agent_builder(request, &binding).await
        }
        SystemMcpKey::AskUser => builtins::dispatch_bound_ask_user(request, &binding).await,
        SystemMcpKey::BrowserTools => {
            browser::dispatch_bound_browser_tools(request, &binding).await
        }
        SystemMcpKey::Notepad => builtins::dispatch_bound_notepad(request, &binding).await,
        SystemMcpKey::MemorySkillReader
        | SystemMcpKey::MemoryCommandReader
        | SystemMcpKey::MemoryPluginReader => {
            builtins::dispatch_bound_memory_reader(system_key, request, &binding).await
        }
        _ => jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "ChatOS internal MCP Provider does not own this System MCP",
        ),
    }
}

fn mcp_management_audit_action(request: &JsonRpcRequest) -> (String, String) {
    if request.method == METHOD_TOOLS_CALL {
        let target = request
            .params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown_tool");
        return ("call".to_string(), target.to_string());
    }
    if request.method == METHOD_TOOLS_LIST {
        return ("list".to_string(), "tools".to_string());
    }
    (request.method.clone(), "unsupported_method".to_string())
}

fn require_mcp_management_request(
    headers: &HeaderMap,
) -> Result<ChatosInternalRequestIdentity, String> {
    let config = Config::try_get()?;
    let secret = config
        .mcp_management_internal_api_secret
        .as_deref()
        .ok_or_else(|| "MCP Management ChatOS internal secret is not configured".to_string())?;
    let caller = header_text(headers, "x-chatos-caller")
        .ok_or_else(|| "x-chatos-caller is required".to_string())?;
    if caller != MCP_MANAGEMENT_CALLER {
        return Err("ChatOS internal caller is not allowed".to_string());
    }
    let token = header_text(headers, "x-chatos-internal-token")
        .ok_or_else(|| "x-chatos-internal-token is required".to_string())?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token.as_str(),
        secret,
        MCP_MANAGEMENT_CALLER,
        CHATOS_TOKEN_AUDIENCE,
        MCP_TOOLS_CALL_SCOPE,
    )?;
    Ok(ChatosInternalRequestIdentity {
        caller_service: MCP_MANAGEMENT_CALLER.to_string(),
        audience_service: CHATOS_TOKEN_AUDIENCE.to_string(),
        scope: MCP_TOOLS_CALL_SCOPE.to_string(),
        trace_id: claims.trace_id,
    })
}

fn mcp_management_binding_from_headers(
    headers: &HeaderMap,
) -> Result<McpManagementBinding, String> {
    let required =
        |key: &'static str| header_text(headers, key).ok_or_else(|| format!("{key} is required"));
    let owner_user_id = required("x-mcp-management-owner-user-id")?;
    let agent_key_text = required("x-mcp-management-agent-key")?;
    let agent_key = SystemAgentKey::ALL
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
        turn_id: header_text(headers, "x-mcp-management-turn-id"),
        source_session_id: header_text(headers, "x-mcp-management-source-session-id"),
        source_user_message_id: header_text(headers, "x-mcp-management-source-user-message-id"),
        contact_agent_id: header_text(headers, "x-mcp-management-contact-agent-id"),
    })
}

fn session_matches_binding(
    session: &crate::models::session::Session,
    binding: &McpManagementBinding,
) -> bool {
    let session_owner = session
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let project_id =
        resolve_session_project_scope(session.project_id.as_deref(), session.metadata.as_ref());
    session.id == binding.source_session_id.as_deref().unwrap_or_default()
        && session_owner == Some(binding.owner_user_id.as_str())
        && session.status.trim() == "active"
        && project_id == binding.project_id
}

fn message_matches_turn(message: &crate::models::message::Message, turn_id: &str) -> bool {
    message.role.trim() == "user"
        && message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("conversation_turn_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            == Some(turn_id)
}

fn is_chatos_agent(agent_key: SystemAgentKey) -> bool {
    matches!(
        agent_key,
        SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
    )
}

fn is_browser_agent(agent_key: SystemAgentKey) -> bool {
    matches!(
        agent_key,
        SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
            | SystemAgentKey::TaskRunnerPlanPhase
            | SystemAgentKey::TaskRunnerRunPhase
    )
}

fn is_notepad_agent(agent_key: SystemAgentKey) -> bool {
    matches!(
        agent_key,
        SystemAgentKey::ChatosConversationAgent
            | SystemAgentKey::ChatosPlanningAgent
            | SystemAgentKey::ProjectRequirementExecutionPlannerAgent
            | SystemAgentKey::TaskRunnerPlanPhase
            | SystemAgentKey::TaskRunnerRunPhase
    )
}

#[cfg(test)]
mod tests;
