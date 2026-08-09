// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::shared_cloud_browser_runtime::{
    call_cloud_browser_tool, close_cloud_browser_runtime, probe_cloud_browser_tools,
};

pub(super) async fn close_bound_cloud_browser_session(
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let identity = match require_mcp_management_request(&headers) {
        Ok(identity) => identity,
        Err(message) => return Json(jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, message)),
    };
    let response =
        dispatch_close_bound_cloud_browser_session(session_id.as_str(), &headers, request).await;
    record_chatos_internal_resource_access(
        &identity,
        ChatosInternalResourceAudit {
            represented_user_id: header_text(&headers, "x-mcp-management-owner-user-id").as_deref(),
            project_id: header_text(&headers, "x-mcp-management-project-id").as_deref(),
            resource_type: "browser_runtime_session",
            resource_id: session_id.as_str(),
            resource_name: Some("browser_tools"),
            action: "close",
            outcome: jsonrpc_outcome(&response),
        },
    );
    Json(response)
}

pub(super) async fn dispatch_close_bound_cloud_browser_session(
    session_id: &str,
    headers: &HeaderMap,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    let binding = match mcp_management_binding_from_headers(headers) {
        Ok(binding) => binding,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message),
    };
    if request.method != CLOUD_BROWSER_SESSION_CLOSE_METHOD {
        return jsonrpc_error(
            id,
            MCP_ERROR_METHOD_NOT_FOUND,
            "cloud Browser Runtime close endpoint only accepts browser/session/close",
        );
    }
    if session_id.trim() != binding.session_id {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "cloud Browser Runtime close path does not match the bound Runtime Session",
        );
    }
    if !is_browser_agent(binding.agent_key) {
        return jsonrpc_error(
            id,
            MCP_ERROR_AUTH_REQUIRED,
            "configured Agent is not allowed to close ChatOS Browser Tools MCP",
        );
    }
    if binding.sandbox_target.is_some() {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "sandbox-bound Browser Runtime sessions must be closed through Sandbox Manager",
        );
    }
    let browser_binding = match cloud_browser_binding(&binding) {
        Ok(binding) => binding,
        Err(message) => return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, message),
    };
    match close_cloud_browser_runtime(&browser_binding).await {
        Ok(closed) => jsonrpc_ok(id, serde_json::json!({"closed": closed})),
        Err(error) => jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, error),
    }
}

pub(super) async fn dispatch_bound_browser_tools(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    if binding.sandbox_target.is_some() {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "sandbox-bound Browser Tools must execute through the authorized Sandbox Browser Runtime",
        );
    }
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

pub(super) async fn dispatch_bound_browser_tools_list(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    if binding.sandbox_target.is_some() {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "sandbox-bound Browser Tools catalog must come from the authorized Sandbox Browser Runtime",
        );
    }
    let browser_binding = match resolve_bound_cloud_browser(binding).await {
        Ok(binding) => binding,
        Err((code, message)) => return jsonrpc_error(id, code, message),
    };
    match probe_cloud_browser_tools(&browser_binding) {
        Ok(tools) => jsonrpc_ok(id, serde_json::json!({"tools": tools})),
        Err(error) => jsonrpc_error(id, MCP_ERROR_INTERNAL, error),
    }
}

pub(super) async fn dispatch_bound_browser_execution_authorization(
    request: JsonRpcRequest,
    binding: &McpManagementBinding,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    if let Err((code, message)) = validate_bound_browser_access(binding).await {
        return jsonrpc_error(id, code, message);
    }
    let target = match binding.sandbox_target.as_ref() {
        Some(target) => target,
        None => {
            return jsonrpc_error(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "Sandbox Browser Runtime authorization requires an immutable sandbox target",
            )
        }
    };
    if !is_valid_cloud_sandbox_browser_target(target) {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "Sandbox Browser Runtime target is not a valid immutable Cloud sandbox binding",
        );
    }
    let operation = request
        .params
        .get("operation")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let valid_operation = match operation {
        METHOD_TOOLS_LIST | CLOUD_BROWSER_SESSION_CLOSE_METHOD => true,
        METHOD_TOOLS_CALL => request
            .params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|name| name.starts_with("browser_")),
        _ => false,
    };
    if !valid_operation {
        return jsonrpc_error(
            id,
            MCP_ERROR_INVALID_PARAMS,
            "Sandbox Browser Runtime authorization operation is not allowed",
        );
    }
    jsonrpc_ok(
        id,
        serde_json::json!({
            "authorized": true,
            "target_ref": target.provider_ref(),
        }),
    )
}

pub(super) fn is_valid_cloud_sandbox_browser_target(
    target: &chatos_mcp_management_sdk::SandboxExecutionTarget,
) -> bool {
    target.provider == SandboxProviderKind::Cloud
        && !target.sandbox_id.trim().is_empty()
        && !target.lease_id.trim().is_empty()
        && target.pairing_id.is_none()
        && if target.is_environment {
            target
                .service_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
        } else {
            target.service_id.is_none()
        }
}

async fn resolve_bound_cloud_browser(
    binding: &McpManagementBinding,
) -> Result<CloudBrowserRuntimeBinding, (i32, String)> {
    validate_bound_browser_access(binding).await?;
    cloud_browser_binding(binding).map_err(|message| (MCP_ERROR_INVALID_PARAMS, message))
}

async fn validate_bound_browser_access(
    binding: &McpManagementBinding,
) -> Result<(), (i32, String)> {
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
    Ok(())
}

pub(super) fn cloud_browser_binding(
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
