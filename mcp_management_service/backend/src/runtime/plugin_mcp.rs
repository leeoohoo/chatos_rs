// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    PluginComponentDescriptor, PluginExecutionHost, PluginMcpServer,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMcpRuntimeBinding {
    pub provider_ref: String,
    pub resource_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub normalized_manifest_sha256: String,
    pub component_key: String,
    pub component_content_sha256: String,
    pub declared_execution_host: PluginExecutionHost,
    pub installation_device_id: Option<String>,
    pub permission_snapshot: Vec<String>,
    pub auth_connection_ids: Vec<String>,
    pub runtime: PluginMcpServer,
    pub server_key: Option<String>,
    pub tool_allowlist: Vec<String>,
    pub tool_blocklist: Vec<String>,
    pub required: bool,
    pub allow_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginLocalProviderBinding {
    pub runtime: PluginMcpRuntimeBinding,
    pub run_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub adapter_session_id: String,
    pub operation: String,
    pub session_sha256: String,
    pub tool_snapshot_sha256: String,
    pub tools: Vec<Value>,
    pub oauth_connection_id: Option<String>,
    pub expires_at_unix: i64,
}

impl PluginLocalProviderBinding {
    pub fn publishes_tool(&self, tool_name: &str) -> bool {
        let tool_name = tool_name.trim();
        !tool_name.is_empty()
            && self
                .tools
                .iter()
                .any(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginToolComponentRuntimeBinding {
    pub provider_ref: String,
    pub resource_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub normalized_manifest_sha256: String,
    pub component: PluginComponentDescriptor,
    pub component_content_sha256: String,
    pub installation_device_id: Option<String>,
    pub permission_snapshot: Vec<String>,
    pub auth_connection_ids: Vec<String>,
    pub required: bool,
    pub allow_writes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_arguments: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginLocalToolComponentBinding {
    pub runtime: PluginToolComponentRuntimeBinding,
    pub run_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub adapter_session_id: String,
    pub operation: String,
    pub session_sha256: String,
    pub tools: Vec<Value>,
    #[serde(default)]
    pub instruction_items: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_result: Option<Value>,
    pub expires_at_unix: i64,
}

impl PluginLocalToolComponentBinding {
    pub fn publishes_tool(&self, tool_name: &str) -> bool {
        let tool_name = tool_name.trim();
        !tool_name.is_empty()
            && self
                .tools
                .iter()
                .any(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
    }
}
