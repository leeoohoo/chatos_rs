// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_mcp::{
    AgentBuilderOptions, AgentBuilderService, AgentBuilderStoreRef, AskUserOptions, AskUserService,
    AskUserStoreRef, MemoryCommandReaderOptions, MemoryCommandReaderService,
    MemoryPluginReaderOptions, MemoryPluginReaderService, MemoryReaderStoreRef,
    MemorySkillReaderOptions, MemorySkillReaderService, NotepadBuiltinService, NotepadOptions,
    NotepadStoreRef, SystemMcpKey,
};
use chatos_mcp_service::{
    jsonrpc_error, jsonrpc_ok, JsonRpcRequest, JsonRpcResponse, MCP_ERROR_AUTH_REQUIRED,
    MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS, MCP_ERROR_METHOD_NOT_FOUND, METHOD_TOOLS_CALL,
    METHOD_TOOLS_LIST,
};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::config::Config;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::modules::conversation_runtime::session_scope::resolve_session_project_scope;
use crate::services::shared_builtin_agent_builder::ChatosAgentBuilderStore;
use crate::services::shared_builtin_ask_user::ChatosAskUserStore;
use crate::services::shared_builtin_memory_readers::ChatosMemoryReaderStore;
use crate::services::shared_builtin_notepad::ChatosNotepadStore;
use crate::services::shared_cloud_browser_runtime::{
    call_cloud_browser_tool, close_cloud_browser_runtime, probe_cloud_browser_tools,
    CloudBrowserRuntimeBinding,
};
use crate::services::{chatos_agents, chatos_sessions};

const MCP_MANAGEMENT_CALLER: &str = "mcp-management-service";
const CHATOS_TOKEN_AUDIENCE: &str = "chatos";
const MCP_TOOLS_CALL_SCOPE: &str = "mcp.tools.call";
const CLOUD_BROWSER_SESSION_CLOSE_METHOD: &str = "browser/session/close";
const ASK_USER_SESSION_EXPIRY_SAFETY_MARGIN_MS: u64 = 5 * 60 * 1_000;

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
            post(close_bound_cloud_browser_session),
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
    if let Err(message) = require_mcp_management_request(&headers) {
        return Json(jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, message));
    }
    let binding = match mcp_management_binding_from_headers(&headers) {
        Ok(binding) => binding,
        Err(message) => return Json(jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message)),
    };
    let system_key = match system_key.parse::<SystemMcpKey>() {
        Ok(system_key) => system_key,
        Err(message) => return Json(jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message)),
    };
    if system_key == SystemMcpKey::BrowserTools && request.method == METHOD_TOOLS_LIST {
        return Json(dispatch_bound_browser_tools_list(request, &binding).await);
    }
    if request.method != METHOD_TOOLS_CALL {
        return Json(jsonrpc_error(
            id,
            MCP_ERROR_METHOD_NOT_FOUND,
            "ChatOS internal MCP Provider only accepts tools/call, plus tools/list for Browser Tools",
        ));
    }
    let response = match system_key {
        SystemMcpKey::AgentBuilder => dispatch_bound_agent_builder(request, &binding).await,
        SystemMcpKey::AskUser => dispatch_bound_ask_user(request, &binding).await,
        SystemMcpKey::BrowserTools => dispatch_bound_browser_tools(request, &binding).await,
        SystemMcpKey::Notepad => dispatch_bound_notepad(request, &binding).await,
        SystemMcpKey::MemorySkillReader
        | SystemMcpKey::MemoryCommandReader
        | SystemMcpKey::MemoryPluginReader => {
            dispatch_bound_memory_reader(system_key, request, &binding).await
        }
        _ => jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "ChatOS internal MCP Provider does not own this System MCP",
        ),
    };
    Json(response)
}

async fn close_bound_cloud_browser_session(
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.unwrap_or(Value::Null);
    if let Err(message) = require_mcp_management_request(&headers) {
        return Json(jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, message));
    }
    let binding = match mcp_management_binding_from_headers(&headers) {
        Ok(binding) => binding,
        Err(message) => return Json(jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message)),
    };
    if request.method != CLOUD_BROWSER_SESSION_CLOSE_METHOD {
        return Json(jsonrpc_error(
            id,
            MCP_ERROR_METHOD_NOT_FOUND,
            "cloud Browser Runtime close endpoint only accepts browser/session/close",
        ));
    }
    if session_id.trim() != binding.session_id {
        return Json(jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "cloud Browser Runtime close path does not match the bound Runtime Session",
        ));
    }
    if !is_browser_agent(binding.agent_key) {
        return Json(jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "configured Agent is not allowed to close ChatOS Browser Tools MCP",
        ));
    }
    let browser_binding = match cloud_browser_binding(&binding) {
        Ok(binding) => binding,
        Err(message) => return Json(jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message)),
    };
    match close_cloud_browser_runtime(&browser_binding).await {
        Ok(closed) => Json(jsonrpc_ok(id, serde_json::json!({"closed": closed}))),
        Err(error) => Json(jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, error)),
    }
}

async fn dispatch_bound_browser_tools(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    let browser_binding = match resolve_bound_cloud_browser(binding).await {
        Ok(binding) => binding,
        Err((code, message)) => return jsonrpc_error(id, code, message),
    };
    let Some(name) = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "Browser Tools tool name is required",
        );
    };
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Err(message) = reject_browser_identity_overrides(&arguments) {
        return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message);
    }
    match call_cloud_browser_tool(browser_binding, name, arguments).await {
        Ok(result) => jsonrpc_ok(id, result),
        Err(error) => jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    }
}

async fn dispatch_bound_browser_tools_list(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    let browser_binding = match resolve_bound_cloud_browser(binding).await {
        Ok(binding) => binding,
        Err((code, message)) => return jsonrpc_error(id, code, message),
    };
    match probe_cloud_browser_tools(&browser_binding) {
        Ok(tools) => jsonrpc_ok(id, serde_json::json!({"tools": tools})),
        Err(error) => jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    }
}

async fn resolve_bound_cloud_browser(
    binding: &McpManagementBinding,
) -> Result<CloudBrowserRuntimeBinding, (i32, String)> {
    if !is_browser_agent(binding.agent_key) {
        return Err((
            MCP_ERROR_AUTH_REQUIRED,
            "configured Agent is not allowed to use ChatOS Browser Tools MCP".to_string(),
        ));
    }
    let conversation_id = binding.source_session_id.as_deref().ok_or_else(|| {
        (
            MCP_ERROR_INVALID_PARAMS,
            "ChatOS Browser Tools requires bound source_session_id".to_string(),
        )
    })?;
    let session = chatos_sessions::get_session_by_id(conversation_id)
        .await
        .map_err(|error| (MCP_ERROR_INTERNAL, error))?
        .ok_or_else(|| {
            (
                MCP_ERROR_INTERNAL,
                "bound ChatOS session was not found".to_string(),
            )
        })?;
    if !session_matches_binding(&session, binding) {
        return Err((
            MCP_ERROR_AUTH_REQUIRED,
            "bound ChatOS session does not match MCP Management owner, project, or active scope"
                .to_string(),
        ));
    }
    cloud_browser_binding(binding).map_err(|message| (MCP_ERROR_INVALID_PARAMS, message))
}

async fn dispatch_bound_agent_builder(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    if !is_chatos_agent(binding.agent_key) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "configured Agent is not allowed to use ChatOS Agent Builder MCP",
        );
    }
    let conversation_id = match binding.source_session_id.as_deref() {
        Some(value) => value,
        None => {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "ChatOS Agent Builder requires bound source_session_id",
            )
        }
    };
    let session = match chatos_sessions::get_session_by_id(conversation_id).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return jsonrpc_error(id, MCP_ERROR_INTERNAL, "bound ChatOS session was not found")
        }
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    if !session_matches_binding(&session, binding) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "bound ChatOS session does not match MCP Management owner or project scope",
        );
    }
    let Some(name) = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "Agent Builder tool name is required",
        );
    };
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Err(message) = reject_agent_builder_identity_overrides(&arguments) {
        return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message);
    }
    if name == "update_memory_agent" {
        let Some(agent_id) = arguments
            .get("agent_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "Agent Builder update requires agent_id",
            );
        };
        match chatos_agents::get_agent(agent_id).await {
            Ok(Some(agent)) if agent.user_id.trim() == binding.owner_user_id => {}
            Ok(Some(_)) => {
                return jsonrpc_error(
                    id,
                    MCP_ERROR_AUTH_REQUIRED,
                    "Agent Builder cannot update an agent owned by another user",
                )
            }
            Ok(None) => {}
            Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
        }
    }
    let store = match ChatosAgentBuilderStore::new(binding.owner_user_id.as_str()) {
        Ok(store) => store,
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    let service = match AgentBuilderService::new(AgentBuilderOptions {
        server_name: chatos_mcp::system_mcp_descriptor(SystemMcpKey::AgentBuilder)
            .server_name
            .to_string(),
        user_id: Some(binding.owner_user_id.clone()),
        store: Some(AgentBuilderStoreRef::new(Arc::new(store))),
    }) {
        Ok(service) => service,
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    match service.call_tool(
        name,
        arguments,
        Some(conversation_id),
        binding.turn_id.as_deref(),
        None,
    ) {
        Ok(result) => jsonrpc_ok(id, result),
        Err(error) => jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    }
}

async fn dispatch_bound_notepad(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    if !is_notepad_agent(binding.agent_key) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "configured Agent is not allowed to use ChatOS Notepad MCP",
        );
    }
    let Some(name) = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "Notepad tool name is required",
        );
    };
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Err(message) = reject_notepad_identity_overrides(&arguments) {
        return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message);
    }
    let store = match ChatosNotepadStore::new(binding.owner_user_id.as_str()) {
        Ok(store) => store,
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    let service = match NotepadBuiltinService::new(NotepadOptions {
        server_name: chatos_mcp::system_mcp_descriptor(SystemMcpKey::Notepad)
            .server_name
            .to_string(),
        store: NotepadStoreRef::new(Arc::new(store)),
    }) {
        Ok(service) => service,
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    match service.call_tool(name, arguments) {
        Ok(result) => jsonrpc_ok(id, result),
        Err(error) => jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    }
}

async fn dispatch_bound_memory_reader(
    system_key: SystemMcpKey,
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    if !is_chatos_agent(binding.agent_key) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "configured Agent is not allowed to use ChatOS Memory Reader MCP",
        );
    }
    let conversation_id = match binding.source_session_id.as_deref() {
        Some(value) => value,
        None => {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "ChatOS Memory Reader requires bound source_session_id",
            )
        }
    };
    let contact_agent_id = match binding.contact_agent_id.as_deref() {
        Some(value) => value,
        None => {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "ChatOS Memory Reader requires bound contact_agent_id",
            )
        }
    };
    let session = match chatos_sessions::get_session_by_id(conversation_id).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return jsonrpc_error(id, MCP_ERROR_INTERNAL, "bound ChatOS session was not found")
        }
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    if !session_matches_binding(&session, binding) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "bound ChatOS session does not match MCP Management owner or project scope",
        );
    }
    let runtime_context = match chatos_agents::get_agent_runtime_context(contact_agent_id).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return jsonrpc_error(
                id,
                MCP_ERROR_AUTH_REQUIRED,
                "bound ChatOS contact agent runtime was not found",
            )
        }
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    if runtime_context.agent_id.trim() != contact_agent_id
        || runtime_context.user_id.trim() != binding.owner_user_id
    {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "bound ChatOS contact agent does not belong to the runtime session owner",
        );
    }
    let Some(name) = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "Memory Reader tool name is required",
        );
    };
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let descriptor = chatos_mcp::system_mcp_descriptor(system_key);
    let store = MemoryReaderStoreRef::new(Arc::new(ChatosMemoryReaderStore));
    let result = match system_key {
        SystemMcpKey::MemorySkillReader => {
            MemorySkillReaderService::new(MemorySkillReaderOptions {
                server_name: descriptor.server_name.to_string(),
                agent_id: contact_agent_id.to_string(),
                store,
            })
            .and_then(|service| service.call_tool(name, arguments))
        }
        SystemMcpKey::MemoryCommandReader => {
            MemoryCommandReaderService::new(MemoryCommandReaderOptions {
                server_name: descriptor.server_name.to_string(),
                agent_id: contact_agent_id.to_string(),
                store,
            })
            .and_then(|service| service.call_tool(name, arguments))
        }
        SystemMcpKey::MemoryPluginReader => {
            MemoryPluginReaderService::new(MemoryPluginReaderOptions {
                server_name: descriptor.server_name.to_string(),
                agent_id: contact_agent_id.to_string(),
                store,
            })
            .and_then(|service| service.call_tool(name, arguments))
        }
        _ => Err("ChatOS internal MCP Provider does not own this Memory Reader".to_string()),
    };
    match result {
        Ok(result) => jsonrpc_ok(id, result),
        Err(error) => jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    }
}

async fn dispatch_bound_ask_user(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    if !is_chatos_agent(binding.agent_key) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "configured Agent is not allowed to use ChatOS Ask User MCP",
        );
    }
    let conversation_id = match binding.source_session_id.as_deref() {
        Some(value) => value,
        None => {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "ChatOS Ask User requires bound source_session_id",
            )
        }
    };
    let turn_id = match binding.turn_id.as_deref() {
        Some(value) => value,
        None => {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "ChatOS Ask User requires bound turn_id",
            )
        }
    };
    let source_user_message_id = match binding.source_user_message_id.as_deref() {
        Some(value) => value,
        None => {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "ChatOS Ask User requires bound source_user_message_id",
            )
        }
    };
    let session = match chatos_sessions::get_session_by_id(conversation_id).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return jsonrpc_error(id, MCP_ERROR_INTERNAL, "bound ChatOS session was not found")
        }
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    if !session_matches_binding(&session, binding) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "bound ChatOS session does not match MCP Management owner or project scope",
        );
    }
    let source_user_message = match chatos_sessions::get_message_by_id_in_session_including_hidden(
        &session,
        source_user_message_id,
    )
    .await
    {
        Ok(Some(message)) => message,
        Ok(None) => {
            return jsonrpc_error(
                id,
                MCP_ERROR_AUTH_REQUIRED,
                "bound ChatOS user message was not found in the bound session",
            )
        }
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    if !message_matches_turn(&source_user_message, turn_id) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "bound ChatOS user message does not match the bound turn",
        );
    }
    let prompt_timeout_ms = match bound_ask_user_prompt_timeout_ms(binding) {
        Ok(timeout_ms) => timeout_ms,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, message),
    };
    let service = match AskUserService::new(AskUserOptions {
        server_name: chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser)
            .server_name
            .to_string(),
        prompt_timeout_ms,
        store: AskUserStoreRef::new(Arc::new(ChatosAskUserStore)),
    }) {
        Ok(service) => service,
        Err(error) => return jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    };
    let Some(name) = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "Ask User tool name is required",
        );
    };
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    match service.call_tool(name, arguments, Some(conversation_id), Some(turn_id), None) {
        Ok(result) => jsonrpc_ok(id, result),
        Err(error) => jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    }
}

fn require_mcp_management_request(headers: &HeaderMap) -> Result<(), String> {
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
    chatos_service_runtime::verify_internal_service_token(
        token.as_str(),
        secret,
        MCP_MANAGEMENT_CALLER,
        CHATOS_TOKEN_AUDIENCE,
        MCP_TOOLS_CALL_SCOPE,
    )?;
    Ok(())
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

fn session_matches_binding(session: &Session, binding: &McpManagementBinding) -> bool {
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

fn message_matches_turn(message: &Message, turn_id: &str) -> bool {
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

fn cloud_browser_binding(
    binding: &McpManagementBinding,
) -> Result<CloudBrowserRuntimeBinding, String> {
    let source_session_id = binding
        .source_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "cloud Browser Runtime requires bound source_session_id".to_string())?;
    Ok(CloudBrowserRuntimeBinding {
        runtime_session_id: binding.session_id.clone(),
        owner_user_id: binding.owner_user_id.clone(),
        agent_key: binding.agent_key,
        project_id: binding.project_id.clone(),
        source_session_id: source_session_id.to_string(),
        expires_at_unix: binding.session_expires_at_unix,
    })
}

#[cfg(test)]
mod tests;
