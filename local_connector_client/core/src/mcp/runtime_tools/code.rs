// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use anyhow::{anyhow, Result};
use chatos_mcp::{CodeMaintainerOptions, CodeMaintainerService};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::{
    WorkspaceState, MAX_LOCAL_MCP_READ_BYTES, MAX_LOCAL_MCP_SEARCH_RESULTS,
    MAX_LOCAL_MCP_WRITE_BYTES,
};

use super::project::normalize_request_task_project_relative_path;
use crate::workspace::paths::request_owned_paths;

pub(crate) fn code_maintainer_service_for_root(
    root: &Path,
    project_id: Option<String>,
    conversation_id: Option<String>,
    run_id: Option<String>,
    allowed_write_paths: Option<Vec<String>>,
    allow_writes: bool,
    enable_read_tools: bool,
    enable_write_tools: bool,
) -> Result<CodeMaintainerService> {
    CodeMaintainerService::new(CodeMaintainerOptions {
        server_name: "local_connector_code_maintainer".to_string(),
        root: root.to_path_buf(),
        project_id,
        allow_writes,
        allowed_write_paths,
        max_file_bytes: MAX_LOCAL_MCP_READ_BYTES as i64,
        max_write_bytes: MAX_LOCAL_MCP_WRITE_BYTES as i64,
        search_limit: MAX_LOCAL_MCP_SEARCH_RESULTS,
        enable_read_tools,
        enable_write_tools,
        conversation_id,
        run_id,
        db_path: None,
        hooks: None,
    })
    .map_err(|err| anyhow!(err))
}

pub(crate) fn code_maintainer_service_for_request(
    root: &Path,
    project_id: Option<String>,
    request: &RelayRequest,
    allow_writes: bool,
    enable_read_tools: bool,
    enable_write_tools: bool,
) -> Result<CodeMaintainerService> {
    let session_id = relay_header(request, "x-mcp-management-session-id")
        .or_else(|| relay_header(request, "x-mcp-management-run-id"))
        .unwrap_or(request.workspace_id.as_str())
        .to_string();
    let run_id = relay_header(request, "x-mcp-management-run-id")
        .or_else(|| relay_header(request, "x-mcp-management-session-id"))
        .unwrap_or(request.workspace_id.as_str())
        .to_string();
    code_maintainer_service_for_root(
        root,
        project_id,
        Some(session_id),
        Some(run_id),
        request_owned_paths(request)?,
        allow_writes,
        enable_read_tools,
        enable_write_tools,
    )
}

fn relay_header<'a>(request: &'a RelayRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn normalize_code_maintainer_arguments(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    tool_name: &str,
    mut arguments: Value,
) -> Result<Value> {
    let Some(map) = arguments.as_object_mut() else {
        return Ok(arguments);
    };
    if tool_name == "stage_edit_batch" {
        if let Some(operations) = map.get_mut("operations").and_then(Value::as_array_mut) {
            for operation in operations {
                let Some(operation) = operation.as_object_mut() else {
                    continue;
                };
                let Some(path) = operation.get("path").and_then(Value::as_str) else {
                    continue;
                };
                let normalized =
                    normalize_request_task_project_relative_path(workspace, request, path)?;
                operation.insert("path".to_string(), Value::String(normalized));
            }
        }
        return Ok(arguments);
    }
    if let Some(path) = map.get("path").and_then(Value::as_str) {
        let normalized = normalize_request_task_project_relative_path(workspace, request, path)?;
        map.insert("path".to_string(), Value::String(normalized));
    }
    Ok(arguments)
}

pub(crate) fn code_maintainer_structured_result(result: Value) -> Value {
    if let Some(payload) = result.get("_structured_result") {
        return payload.clone();
    }
    if let Some(text) = result.pointer("/content/0/text").and_then(Value::as_str) {
        return serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({ "text": text }));
    }
    result
}
