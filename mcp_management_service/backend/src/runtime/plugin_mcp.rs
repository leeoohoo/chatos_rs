// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{ProjectExecutionContext, WorkspaceProviderKind};
use chatos_plugin_management_sdk::{PluginComponentDescriptor, PluginMcpServer};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub adapter_session_id: String,
    pub operation: String,
    pub session_sha256: String,
    pub snapshot_sha256: String,
    pub tool_snapshot_sha256: String,
    pub server_instructions_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_instructions: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginLocalExecutionTarget {
    pub(crate) device_id: String,
    pub(crate) workspace_id: Option<String>,
    pub(crate) project_root: Option<String>,
}

pub(crate) fn resolve_plugin_local_execution_target(
    context: &ProjectExecutionContext,
    installation_device_id: Option<&str>,
    permission_snapshot: &[String],
) -> Result<PluginLocalExecutionTarget, String> {
    let installation_device_id = installation_device_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Plugin installation device is missing".to_string())?;

    match context.workspace_provider {
        WorkspaceProviderKind::LocalConnector => {
            let workspace = context.workspace.as_ref().ok_or_else(|| {
                "Plugin Local route is missing its project workspace snapshot".to_string()
            })?;
            let workspace_device_id = workspace
                .device_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Plugin Local route is missing its device id".to_string())?;
            if workspace_device_id != installation_device_id {
                return Err(
                    "Plugin installation is not pinned to the Project Context device".to_string(),
                );
            }
            let workspace_id = workspace.workspace_id.trim();
            if workspace_id.is_empty() {
                return Err("Plugin Local route is missing its workspace id".to_string());
            }
            Ok(PluginLocalExecutionTarget {
                device_id: installation_device_id.to_string(),
                workspace_id: Some(workspace_id.to_string()),
                project_root: workspace
                    .relative_root
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            })
        }
        WorkspaceProviderKind::None => {
            if context.workspace.is_some() {
                return Err(
                    "Plugin Local route has a workspace without a workspace provider".to_string(),
                );
            }
            if permission_snapshot.iter().any(|permission| {
                permission
                    .trim()
                    .to_ascii_lowercase()
                    .starts_with("workspace.")
            }) {
                return Err(
                    "Plugin requires workspace permissions but no project workspace is available"
                        .to_string(),
                );
            }
            Ok(PluginLocalExecutionTarget {
                device_id: installation_device_id.to_string(),
                workspace_id: None,
                project_root: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use chatos_mcp_management_sdk::{
        ProjectExecutionContext, WorkspaceExecutionTarget, WorkspaceProviderKind,
    };

    use super::resolve_plugin_local_execution_target;

    fn device_only_context() -> ProjectExecutionContext {
        ProjectExecutionContext {
            project_id: None,
            owner_user_id: "owner-1".to_string(),
            workspace_provider: WorkspaceProviderKind::None,
            workspace: None,
            revision: "public-v1".to_string(),
        }
    }

    fn project_context(device_id: &str) -> ProjectExecutionContext {
        ProjectExecutionContext {
            project_id: Some("project-1".to_string()),
            owner_user_id: "owner-1".to_string(),
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some(device_id.to_string()),
                workspace_id: "workspace-1".to_string(),
                relative_root: Some("apps/chatos".to_string()),
            }),
            revision: "project-v1".to_string(),
        }
    }

    #[test]
    fn device_only_plugin_uses_its_installation_device_without_a_workspace() {
        let target = resolve_plugin_local_execution_target(
            &device_only_context(),
            Some("device-1"),
            &["network.domain:github.com".to_string()],
        )
        .expect("device-only Plugin target");

        assert_eq!(target.device_id, "device-1");
        assert_eq!(target.workspace_id, None);
        assert_eq!(target.project_root, None);
    }

    #[test]
    fn workspace_permission_is_rejected_without_a_project_workspace() {
        let error = resolve_plugin_local_execution_target(
            &device_only_context(),
            Some("device-1"),
            &["workspace.read".to_string()],
        )
        .expect_err("workspace permission must fail closed");

        assert!(error.contains("workspace permissions"));
    }

    #[test]
    fn project_workspace_still_pins_the_plugin_installation_device() {
        let target = resolve_plugin_local_execution_target(
            &project_context("device-1"),
            Some("device-1"),
            &["workspace.read".to_string()],
        )
        .expect("project Plugin target");

        assert_eq!(target.workspace_id.as_deref(), Some("workspace-1"));
        assert_eq!(target.project_root.as_deref(), Some("apps/chatos"));

        let error = resolve_plugin_local_execution_target(
            &project_context("device-2"),
            Some("device-1"),
            &["workspace.read".to_string()],
        )
        .expect_err("mismatched project device must fail closed");
        assert!(error.contains("Project Context device"));
    }
}
