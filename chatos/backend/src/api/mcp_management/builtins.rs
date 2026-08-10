// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use super::*;
use crate::services::shared_builtin_agent_builder::ChatosAgentBuilderStore;
use crate::services::shared_builtin_ask_user::ChatosAskUserStore;
use crate::services::shared_builtin_memory_readers::ChatosMemoryReaderStore;
use crate::services::shared_builtin_notepad::ChatosNotepadStore;
use chatos_mcp::{
    AgentBuilderOptions, AgentBuilderService, AgentBuilderStoreRef, AskUserOptions, AskUserService,
    AskUserStoreRef, MemoryCommandReaderOptions, MemoryCommandReaderService,
    MemoryPluginReaderOptions, MemoryPluginReaderService, MemoryReaderStoreRef,
    MemorySkillReaderOptions, MemorySkillReaderService, NotepadBuiltinService, NotepadOptions,
    NotepadStoreRef,
};

pub(super) async fn dispatch_bound_agent_builder(
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

pub(super) async fn dispatch_bound_notepad(
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

pub(super) async fn dispatch_bound_memory_reader(
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

pub(super) async fn dispatch_bound_ask_user(
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
