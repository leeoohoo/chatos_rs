// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::Result;
use chatos_mcp_service::{McpRequestContext, McpToolProvider};
use serde_json::Value;

use crate::history::CommandHistoryRecorder;
use crate::relay::RelayRequest;
use crate::workspace::paths::workspace_for_request;
use crate::LocalState;

use super::selection::{
    is_browser_tool, is_code_maintainer_tool, is_terminal_controller_tool, local_mcp_tool_selection,
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
        _context: McpRequestContext,
    ) -> std::result::Result<Value, String> {
        call_builtin_compatible_local_tool(
            &self.request,
            &self.state,
            name,
            args,
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
        let browser_service =
            local_browser_tools_service_for_root(project_root.as_path(), request)?;
        tools.extend(browser_service.list_tools());
    }
    Ok(tools)
}

pub(crate) async fn call_builtin_compatible_local_tool(
    request: &RelayRequest,
    state: &LocalState,
    name: &str,
    arguments: Value,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Option<Value>> {
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let selection = local_mcp_tool_selection(request);
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
        let result = service
            .call_tool(name, arguments, None)
            .map_err(anyhow::Error::msg)?;
        return Ok(Some(result));
    }
    if is_terminal_controller_tool(name) {
        if !selection.terminal {
            return Ok(None);
        }
        let result = call_local_terminal_controller_tool(
            request,
            state,
            workspace,
            name,
            arguments,
            history_recorder,
        )
        .await?;
        return Ok(Some(result));
    }
    if is_browser_tool(name) {
        if !selection.browser {
            return Ok(None);
        }
        let project_root = request_project_root(workspace, request)?;
        let service = local_browser_tools_service_for_root(project_root.as_path(), request)?;
        let result = service
            .call_tool(
                name,
                arguments,
                Some(local_browser_conversation_id(request).as_str()),
            )
            .map_err(anyhow::Error::msg)?;
        return Ok(Some(result));
    }
    Ok(None)
}
