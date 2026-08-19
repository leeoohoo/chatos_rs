// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use anyhow::Result;
use chatos_mcp_service::{McpRequestContext, McpToolProvider};
use serde_json::{json, Value};

use crate::approval::{
    approval_project_key_from_request, ApprovalDecision, CommandApprovalRequest,
    CommandApprovalService,
};
use crate::history::CommandHistoryRecorder;
use crate::local_runtime::LocalDatabase;
use crate::relay::RelayRequest;
use crate::sandbox::types::LocalSandboxRuntime;
use crate::terminal::controller::local_mcp_terminal_project_id;
use crate::workspace::paths::{
    relative_to_workspace, request_default_tool_root, workspace_for_request,
};
use crate::LocalState;

use super::selection::{
    is_browser_tool, is_code_maintainer_tool, is_local_command_approval_tool,
    is_terminal_controller_tool, local_mcp_tool_selection,
};
use super::tools::{
    call_local_terminal_controller_tool, code_maintainer_service_for_root,
    local_browser_conversation_id, local_browser_tools_service_for_root,
    normalize_code_maintainer_arguments, request_project_root,
};
use crate::terminal::controller::local_terminal_controller_service_for_root;

#[derive(Clone)]
pub(crate) struct LocalConnectorMcpToolProvider {
    pub(crate) request: RelayRequest,
    pub(crate) state: LocalState,
    pub(crate) execution_runtime: Option<(reqwest::Client, LocalSandboxRuntime)>,
    pub(crate) database: Option<LocalDatabase>,
    pub(crate) history_recorder: CommandHistoryRecorder,
}

#[async_trait::async_trait]
impl McpToolProvider for LocalConnectorMcpToolProvider {
    fn server_name(&self) -> &str {
        "local_connector"
    }

    fn list_tools(&self, _context: &McpRequestContext) -> Vec<Value> {
        local_mcp_builtin_compatible_tools(&self.request, &self.state).unwrap_or_default()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: McpRequestContext,
    ) -> std::result::Result<Value, String> {
        call_builtin_compatible_local_tool_with_limit(
            &self.request,
            &self.state,
            name,
            args,
            context.tool_result_max_chars(),
            self.execution_runtime
                .as_ref()
                .map(|(http_client, sandbox_runtime)| (http_client, sandbox_runtime)),
            self.database.as_ref(),
            &self.history_recorder,
        )
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| format!("unsupported local connector tool: {name}"))
    }
}

pub(crate) fn local_mcp_builtin_compatible_tools(
    request: &RelayRequest,
    state: &LocalState,
) -> Result<Vec<Value>> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_root = request_project_root(workspace, request)?;
    let selection = local_mcp_tool_selection(request);
    let mut tools = Vec::new();
    if selection.code_read || selection.code_write {
        let code_service = code_maintainer_service_for_root(
            project_root.as_path(),
            Some(workspace.id.clone()),
            selection.code_write,
            selection.code_read,
            selection.code_write,
        )?;
        tools.extend(code_service.list_tools());
    }
    if selection.terminal {
        let terminal_service =
            local_terminal_controller_service_for_root(project_root.as_path(), request, 60_000)?;
        tools.extend(terminal_service.list_tools());
    }
    if selection.browser {
        let browser_service = local_browser_tools_service_for_root(
            project_root.as_path(),
            request,
            state.runtime_settings.browser_full_cdp_access_enabled,
        )?;
        tools.extend(browser_service.list_tools());
    }
    if selection.local_command_approval {
        tools.push(chatos_mcp::local_command_approval_decision_tool_definition());
    }
    Ok(tools)
}

#[cfg(test)]
pub(crate) async fn call_builtin_compatible_local_tool(
    request: &RelayRequest,
    state: &LocalState,
    name: &str,
    arguments: Value,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Option<Value>> {
    call_builtin_compatible_local_tool_with_limit(
        request,
        state,
        name,
        arguments,
        None,
        None,
        None,
        history_recorder,
    )
    .await
}

async fn call_builtin_compatible_local_tool_with_limit(
    request: &RelayRequest,
    state: &LocalState,
    name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
    execution_runtime: Option<(&reqwest::Client, &LocalSandboxRuntime)>,
    database: Option<&LocalDatabase>,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Option<Value>> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let selection = local_mcp_tool_selection(request);
    if is_local_command_approval_tool(name) {
        if !selection.local_command_approval {
            return Ok(None);
        }
        let (_, result) = crate::approval::approval_decision_tool_result(arguments)
            .map_err(anyhow::Error::msg)?;
        return Ok(Some(result));
    }
    if is_code_maintainer_tool(name) {
        if !selection.allows_code_tool(name) {
            return Ok(None);
        }
        let project_root = request_project_root(workspace, request)?;
        let service = code_maintainer_service_for_root(
            project_root.as_path(),
            Some(workspace.id.clone()),
            selection.code_write,
            selection.code_read,
            selection.code_write,
        )?;
        let arguments = normalize_code_maintainer_arguments(workspace, request, name, arguments)?;
        if default_tool_root_list_target_is_missing(
            request,
            project_root.as_path(),
            name,
            &arguments,
        )? {
            return Ok(Some(code_maintainer_empty_list_dir_result()));
        }
        if let Some((http_client, sandbox_runtime)) = execution_runtime {
            return crate::mcp::execution_scope::call_local_execution_scope_tool(
                request,
                state,
                http_client,
                sandbox_runtime,
                database.ok_or_else(|| {
                    anyhow::anyhow!("local execution scope database is unavailable")
                })?,
                project_root.as_path(),
                name,
                arguments,
                tool_result_max_chars,
            )
            .await
            .map(Some);
        }
        let result = service
            .call_tool(name, arguments, None)
            .map_err(anyhow::Error::msg)?;
        return Ok(Some(result));
    }
    if is_terminal_controller_tool(name) {
        if !selection.terminal {
            return Ok(None);
        }
        let result = if let Some((http_client, sandbox_runtime)) = execution_runtime {
            crate::mcp::execution_scope::call_local_execution_scope_terminal_tool(
                request,
                state,
                http_client,
                sandbox_runtime,
                database.ok_or_else(|| {
                    anyhow::anyhow!("local execution scope database is unavailable")
                })?,
                workspace,
                name,
                arguments,
                tool_result_max_chars,
                history_recorder,
            )
            .await?
        } else {
            call_local_terminal_controller_tool(
                request,
                state,
                workspace,
                name,
                arguments,
                tool_result_max_chars,
                history_recorder,
            )
            .await?
        };
        return Ok(Some(result));
    }
    if is_browser_tool(name) {
        if !selection.browser {
            return Ok(None);
        }
        let project_root = request_project_root(workspace, request)?;
        if matches!(name, "browser_route_add" | "browser_cdp_command") {
            if name == "browser_cdp_command"
                && !state.runtime_settings.browser_full_cdp_access_enabled
            {
                return Ok(Some(browser_approval_denied_result(
                    name,
                    "full browser CDP access is disabled in Local Connector settings",
                )));
            }
            let (command, approval_args) =
                chatos_mcp::browser_interactive_approval_command(name, &arguments)
                    .map_err(anyhow::Error::msg)?;
            let project_key = approval_project_key_from_request(
                state,
                request,
                workspace,
                relative_to_workspace(workspace, project_root.as_path()),
            );
            let approval = CommandApprovalService::new(
                history_recorder.state_path.clone(),
                history_recorder.state.clone(),
            )
            .approve_interactive(CommandApprovalRequest {
                request_id: request.request_id.clone(),
                project_key,
                command,
                args: approval_args,
                redact_arguments_in_history: true,
                cwd: ".".to_string(),
                source: "browser_privileged_action".to_string(),
                requested_permissions: None,
                session_id: Some(local_browser_conversation_id(request)),
                action_audit: None,
            })
            .await?;
            if let ApprovalDecision::Denied { reason, .. } = approval {
                return Ok(Some(browser_approval_denied_result(name, reason.as_str())));
            }
        }
        let service = local_browser_tools_service_for_root(
            project_root.as_path(),
            request,
            state.runtime_settings.browser_full_cdp_access_enabled,
        )?;
        let mut result = service
            .call_tool_with_context(
                name,
                arguments,
                chatos_mcp::BrowserToolCallContext::from_conversation_id(Some(
                    local_browser_conversation_id(request).as_str(),
                ))
                .with_tool_result_max_chars(tool_result_max_chars),
            )
            .map_err(anyhow::Error::msg)?;
        annotate_browser_session_context(
            &mut result,
            request.workspace_id.as_str(),
            state.device_id.as_deref(),
            local_mcp_terminal_project_id(request).as_deref(),
        );
        return Ok(Some(result));
    }
    Ok(None)
}

fn default_tool_root_list_target_is_missing(
    request: &RelayRequest,
    project_root: &Path,
    name: &str,
    arguments: &Value,
) -> Result<bool> {
    if name != "list_dir" {
        return Ok(false);
    }
    let Some(default_root) = request_default_tool_root(request)? else {
        return Ok(false);
    };
    let path = arguments
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(default_root.as_str());
    Ok(path == default_root && !project_root.join(default_root).exists())
}

fn code_maintainer_empty_list_dir_result() -> Value {
    let payload = json!({ "entries": [] });
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
            }
        ],
        "_structured_result": payload
    })
}

fn browser_approval_denied_result(tool_name: &str, reason: &str) -> Value {
    let payload = serde_json::json!({
        "success": false,
        "tool_name": tool_name,
        "error": reason,
        "approval_decision": "denied",
        "approval_reason": reason,
    });
    serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        }],
        "_structured_result": payload,
    })
}

fn annotate_browser_session_context(
    value: &mut Value,
    workspace_id: &str,
    device_id: Option<&str>,
    project_id: Option<&str>,
) {
    match value {
        Value::Object(map) => {
            if let Some(session) = map
                .get_mut("browser_session")
                .and_then(Value::as_object_mut)
            {
                session.insert(
                    "workspace_id".to_string(),
                    Value::String(workspace_id.to_string()),
                );
                if let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty())
                {
                    session.insert(
                        "device_id".to_string(),
                        Value::String(device_id.to_string()),
                    );
                }
                if let Some(project_id) =
                    project_id.map(str::trim).filter(|value| !value.is_empty())
                {
                    session.insert(
                        "project_id".to_string(),
                        Value::String(project_id.to_string()),
                    );
                }
            }
            for child in map.values_mut() {
                annotate_browser_session_context(child, workspace_id, device_id, project_id);
            }
        }
        Value::Array(items) => {
            for child in items {
                annotate_browser_session_context(child, workspace_id, device_id, project_id);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod browser_session_context_tests {
    use super::annotate_browser_session_context;
    use serde_json::json;

    #[test]
    fn browser_session_context_adds_local_routing_identity() {
        let mut value = json!({
            "content": [{"type": "text", "text": "ok"}],
            "_structured_result": {
                "browser_session": {
                    "id": "h_session",
                    "mode": "managed"
                }
            }
        });

        annotate_browser_session_context(
            &mut value,
            "workspace-1",
            Some("device-1"),
            Some("project-1"),
        );

        assert_eq!(
            value["_structured_result"]["browser_session"]["workspace_id"],
            "workspace-1"
        );
        assert_eq!(
            value["_structured_result"]["browser_session"]["device_id"],
            "device-1"
        );
    }
}
