// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::history::CommandHistoryRecorder;
use crate::mcp::selection::local_mcp_tool_selection;
use crate::mcp::tools::{normalize_request_project_relative_path, request_project_root};
use crate::relay::RelayRequest;
use crate::terminal::controller::{
    local_terminal_controller_context_for_root, LocalConnectorTerminalControllerStore,
};
use crate::workspace::paths::workspace_for_request;
use crate::{LocalState, DEFAULT_TERMINAL_EXEC_TIMEOUT_MS};

pub(crate) async fn handle_local_mcp_terminal_start(
    request: &RelayRequest,
    state: &LocalState,
    _history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    if !local_mcp_tool_selection(request).terminal {
        return Err(anyhow!(
            "local connector terminal tools are not enabled for this task"
        ));
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_root = request_project_root(workspace, request)?;
    let body = &request.body;
    let params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    let requested_path = params.get("path").and_then(Value::as_str).unwrap_or(".");
    let normalized_path =
        normalize_request_project_relative_path(workspace, request, requested_path)?;
    let context = local_terminal_controller_context_for_root(
        project_root.as_path(),
        request,
        DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    );
    let payload = LocalConnectorTerminalControllerStore
        .start_shell_session(context, normalized_path)
        .await
        .map_err(|err| anyhow!(err))?;
    Ok(json!({
        "jsonrpc": "2.0",
        "id": body.get("id").cloned().unwrap_or(Value::Null),
        "result": payload
    }))
}

pub(crate) async fn handle_local_mcp_terminal_cleanup(
    request: &RelayRequest,
    state: &LocalState,
) -> Result<Value> {
    if !local_mcp_tool_selection(request).terminal {
        return Ok(json!({
            "jsonrpc": "2.0",
            "id": request.body.get("id").cloned().unwrap_or(Value::Null),
            "result": {
                "ok": true,
                "total": 0,
                "killed": 0,
                "already_exited": 0,
                "terminal_ids": [],
                "errors": [],
                "skipped": "terminal tools are not enabled for this task"
            }
        }));
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_root = request_project_root(workspace, request)?;
    let body = &request.body;
    let context = local_terminal_controller_context_for_root(
        project_root.as_path(),
        request,
        DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    );
    let payload = LocalConnectorTerminalControllerStore
        .kill_sessions_for_context(context)
        .await
        .map_err(|err| anyhow!(err))?;
    Ok(json!({
        "jsonrpc": "2.0",
        "id": body.get("id").cloned().unwrap_or(Value::Null),
        "result": payload
    }))
}
