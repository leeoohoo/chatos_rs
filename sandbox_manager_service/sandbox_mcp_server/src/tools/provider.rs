// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_mcp::{
    extract_patch_targets, CodeMaintainerService, TerminalControllerService,
};
use chatos_mcp_service::{sort_tools_by_name, tool_name_set, McpRequestContext, McpToolProvider};
use serde_json::Value;

use crate::command_sandbox::FileToolAccessPolicy;
use crate::quota::WorkspaceQuota;

#[derive(Clone)]
pub struct SandboxMcpToolProvider {
    file_service: CodeMaintainerService,
    terminal_service: TerminalControllerService,
    file_tool_names: HashSet<String>,
    terminal_tool_names: HashSet<String>,
    tools: Vec<Value>,
    workspace_quota: WorkspaceQuota,
    file_access_policy: Arc<FileToolAccessPolicy>,
}

impl SandboxMcpToolProvider {
    pub fn new(
        file_service: CodeMaintainerService,
        terminal_service: TerminalControllerService,
        workspace_quota: WorkspaceQuota,
        file_access_policy: Arc<FileToolAccessPolicy>,
    ) -> Self {
        let file_tools = sort_tools_by_name(file_service.list_tools());
        let terminal_tools = sort_tools_by_name(terminal_service.list_tools());
        let file_tool_names = tool_name_set(&file_tools);
        let terminal_tool_names = tool_name_set(&terminal_tools);
        let tools = sort_tools_by_name(file_tools.into_iter().chain(terminal_tools).collect());
        Self {
            file_service,
            terminal_service,
            file_tool_names,
            terminal_tool_names,
            tools,
            workspace_quota,
            file_access_policy,
        }
    }

    pub fn tools(&self) -> Vec<Value> {
        self.tools.clone()
    }
}

#[async_trait]
impl McpToolProvider for SandboxMcpToolProvider {
    fn server_name(&self) -> &str {
        "chatos-sandbox-mcp-server"
    }

    fn list_tools(&self, _context: &McpRequestContext) -> Vec<Value> {
        self.tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: McpRequestContext,
    ) -> Result<Value, String> {
        self.workspace_quota.check().await?;
        let result = if self.file_tool_names.contains(name) {
            authorize_file_tool_call(self.file_access_policy.as_ref(), name, &args)?;
            self.file_service.call_tool(name, args, None)
        } else if self.terminal_tool_names.contains(name) {
            self.terminal_service.call_tool(name, args, None)
        } else {
            return Err(format!("tool not found: {name}"));
        };
        self.workspace_quota.check().await?;
        result
    }
}

fn authorize_file_tool_call(
    policy: &FileToolAccessPolicy,
    name: &str,
    args: &Value,
) -> Result<(), String> {
    match name {
        "read_file_raw" | "read_file_range" | "read_file" | "list_dir" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
            let path = policy.resolve_workspace_path(path)?;
            policy.authorize_read(path.as_path())
        }
        "search_text" | "search_files" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
            let path = policy.resolve_workspace_path(path)?;
            if path.is_file() {
                policy.authorize_read(path.as_path())
            } else {
                policy.authorize_recursive_read(path.as_path())
            }
        }
        "write_file" | "edit_file" | "append_file" => {
            let path = required_file_tool_path(args)?;
            let path = policy.resolve_workspace_path(path)?;
            policy.authorize_write(path.as_path())
        }
        "delete_path" => {
            let path = required_file_tool_path(args)?;
            let path = policy.resolve_workspace_path(path)?;
            if path.is_dir() {
                policy.authorize_recursive_write(path.as_path())
            } else {
                policy.authorize_write(path.as_path())
            }
        }
        "apply_patch" | "patch" => {
            let patch = args
                .get("patch")
                .and_then(Value::as_str)
                .ok_or_else(|| "patch is required".to_string())?;
            for target in extract_patch_targets(patch) {
                let before = policy.resolve_workspace_path(target.before_path.as_str())?;
                policy.authorize_write(before.as_path())?;
                let after = policy.resolve_workspace_path(target.after_path.as_str())?;
                policy.authorize_write(after.as_path())?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn required_file_tool_path(args: &Value) -> Result<&str, String> {
    args.get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "path is required".to_string())
}
