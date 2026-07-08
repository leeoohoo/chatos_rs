// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use anyhow::{anyhow, Result};
use chatos_builtin_tools::{CodeMaintainerOptions, CodeMaintainerService};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::{
    WorkspaceState, MAX_LOCAL_MCP_READ_BYTES, MAX_LOCAL_MCP_SEARCH_RESULTS,
    MAX_LOCAL_MCP_WRITE_BYTES,
};

use super::project::normalize_request_project_relative_path;

pub(crate) fn code_maintainer_service_for_root(
    root: &Path,
    project_id: Option<String>,
    allow_writes: bool,
    enable_read_tools: bool,
    enable_write_tools: bool,
) -> Result<CodeMaintainerService> {
    CodeMaintainerService::new(CodeMaintainerOptions {
        server_name: "local_connector_code_maintainer".to_string(),
        root: root.to_path_buf(),
        project_id,
        allow_writes,
        max_file_bytes: MAX_LOCAL_MCP_READ_BYTES as i64,
        max_write_bytes: MAX_LOCAL_MCP_WRITE_BYTES as i64,
        search_limit: MAX_LOCAL_MCP_SEARCH_RESULTS,
        enable_read_tools,
        enable_write_tools,
        conversation_id: None,
        run_id: None,
        db_path: None,
        hooks: None,
    })
    .map_err(|err| anyhow!(err))
}

pub(crate) fn normalize_code_maintainer_arguments(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    tool_name: &str,
    mut arguments: Value,
) -> Result<Value> {
    if matches!(tool_name, "apply_patch" | "patch") {
        return Ok(arguments);
    }
    let Some(map) = arguments.as_object_mut() else {
        return Ok(arguments);
    };
    if let Some(path) = map.get("path").and_then(Value::as_str) {
        let normalized = normalize_request_project_relative_path(workspace, request, path)?;
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
