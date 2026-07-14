// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct DeviceQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct WorkspaceQuery {
    pub(super) device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalFsQuery {
    pub(super) device_id: Option<String>,
    pub(super) workspace_id: Option<String>,
    pub(super) path: Option<String>,
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateLocalDirectoryRequest {
    pub(super) device_id: Option<String>,
    pub(super) workspace_id: Option<String>,
    pub(super) path: Option<String>,
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateLocalConnectorProjectRequest {
    pub(super) name: Option<String>,
    pub(super) device_id: Option<String>,
    pub(super) workspace_id: Option<String>,
    pub(super) relative_path: Option<String>,
    pub(super) git_url: Option<String>,
    pub(super) description: Option<String>,
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalTerminalExecRequest {
    pub(super) device_id: Option<String>,
    pub(super) workspace_id: Option<String>,
    pub(super) command: Option<String>,
    pub(super) args: Option<Vec<String>>,
    pub(super) cwd: Option<String>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) source: Option<String>,
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct LocalConnectorDevice {
    pub(super) id: String,
    pub(super) owner_user_id: String,
    pub(super) display_name: String,
    pub(super) public_key: String,
    pub(super) client_version: Option<String>,
    pub(super) os: Option<String>,
    pub(super) status: String,
    pub(super) last_seen_at: Option<String>,
    pub(super) revoked_at: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct LocalConnectorWorkspace {
    pub(super) id: String,
    pub(super) owner_user_id: String,
    pub(super) device_id: String,
    pub(super) display_name: String,
    pub(super) local_path_alias: String,
    pub(super) local_path_fingerprint: String,
    pub(super) capabilities: Vec<String>,
    pub(super) status: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct LocalConnectorProjectBinding {
    pub(super) id: String,
    pub(super) owner_user_id: String,
    pub(super) project_id: String,
    pub(super) device_id: String,
    pub(super) workspace_id: String,
    pub(super) mode: String,
    pub(super) enabled: bool,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct CreateProjectBindingRequest<'a> {
    pub(super) project_id: &'a str,
    pub(super) device_id: &'a str,
    pub(super) workspace_id: &'a str,
    pub(super) mode: &'a str,
    pub(super) enabled: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct UpdateProjectBindingRequest<'a> {
    pub(super) device_id: &'a str,
    pub(super) workspace_id: &'a str,
    pub(super) enabled: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct RelayTerminalExecRequest<'a> {
    pub(super) workspace_id: &'a str,
    pub(super) command: &'a str,
    pub(super) args: Vec<String>,
    pub(super) cwd: Option<String>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) source: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RelayTerminalSessionCreateRequest<'a> {
    pub(super) workspace_id: &'a str,
    pub(super) terminal_session_id: &'a str,
    pub(super) cwd: Option<&'a str>,
    pub(super) cols: u16,
    pub(super) rows: u16,
}

#[derive(Debug, Serialize)]
pub(super) struct RelayTerminalInputRequest<'a> {
    pub(super) workspace_id: &'a str,
    pub(super) terminal_session_id: &'a str,
    pub(super) data: &'a str,
}

#[derive(Debug, Serialize)]
pub(super) struct McpToolCallRequest<'a> {
    pub(super) jsonrpc: &'static str,
    pub(super) id: &'static str,
    pub(super) method: &'static str,
    pub(super) params: McpToolCallParams<'a>,
}

#[derive(Debug, Serialize)]
pub(super) struct McpToolCallParams<'a> {
    pub(super) name: &'a str,
    pub(super) arguments: Value,
}
