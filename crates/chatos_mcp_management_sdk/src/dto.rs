// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPlane {
    #[default]
    Cloud,
    Local,
}

impl ExecutionPlane {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Local => "local",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceProviderKind {
    LocalConnector,
    Harness,
    CloudSandbox,
    CloudStorage,
    #[default]
    None,
}

impl WorkspaceProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalConnector => "local_connector",
            Self::Harness => "harness",
            Self::CloudSandbox => "cloud_sandbox",
            Self::CloudStorage => "cloud_storage",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProviderKind {
    LocalConnector,
    Cloud,
    #[default]
    None,
}

impl SandboxProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalConnector => "local_connector",
            Self::Cloud => "cloud",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceExecutionTarget {
    pub device_id: Option<String>,
    pub workspace_id: String,
    pub relative_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxExecutionTarget {
    pub sandbox_id: String,
    pub lease_id: String,
    #[serde(default)]
    pub is_environment: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
}

impl SandboxExecutionTarget {
    pub fn provider_ref(&self) -> String {
        format!(
            "sandbox:{}/lease:{}",
            self.sandbox_id.trim(),
            self.lease_id.trim()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectExecutionContext {
    pub project_id: String,
    pub owner_user_id: String,
    #[serde(default)]
    pub execution_plane: ExecutionPlane,
    #[serde(default)]
    pub workspace_provider: WorkspaceProviderKind,
    pub workspace: Option<WorkspaceExecutionTarget>,
    #[serde(default)]
    pub sandbox_provider: SandboxProviderKind,
    pub sandbox_pairing_id: Option<String>,
    pub source_type: Option<String>,
    pub revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpProviderKind {
    Embedded,
    InternalService,
    LocalConnector,
    Harness,
    CloudSandbox,
    ExternalHttp,
    CloudStdio,
    PluginLocal,
    PluginCloud,
    Unavailable,
}

impl McpProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::InternalService => "internal_service",
            Self::LocalConnector => "local_connector",
            Self::Harness => "harness",
            Self::CloudSandbox => "cloud_sandbox",
            Self::ExternalHttp => "external_http",
            Self::CloudStdio => "cloud_stdio",
            Self::PluginLocal => "plugin_local",
            Self::PluginCloud => "plugin_cloud",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRouteResourceKind {
    System,
    ExternalHttp,
    Stdio,
    Plugin,
    LocalConnector,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpExecutionHost {
    Cloud,
    Local,
    Portable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRetryClass {
    IdempotentRead,
    ProviderDeclared,
    NoRetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRouteCandidate {
    pub resource_id: String,
    pub server_name: String,
    pub resource_kind: McpRouteResourceKind,
    pub system_key: Option<String>,
    pub execution_host: Option<McpExecutionHost>,
    pub provider_ref: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub allow_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveMcpRoutesRequest {
    pub context: ProjectExecutionContext,
    #[serde(default)]
    pub resources: Vec<McpRouteCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedMcpRoute {
    pub resource_id: String,
    pub server_name: String,
    pub provider_kind: McpProviderKind,
    pub provider_ref: Option<String>,
    pub tool_namespace: String,
    pub allow_writes: bool,
    pub retry_class: McpRetryClass,
    pub cancel_supported: bool,
    pub reason: String,
}

impl ResolvedMcpRoute {
    pub fn is_available(&self) -> bool {
        self.provider_kind != McpProviderKind::Unavailable
    }

    pub fn exposed_tool_name(&self, original_tool_name: &str) -> String {
        format!("{}_{}", self.tool_namespace, original_tool_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveMcpRoutesResponse {
    pub project_revision: String,
    pub route_revision: String,
    pub routes: Vec<ResolvedMcpRoute>,
    #[serde(default)]
    pub unavailable_required_mcps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCatalogItem {
    pub resource_id: String,
    pub system_key: String,
    pub server_name: String,
    pub display_name: String,
    pub description: String,
    pub owner_service: String,
    pub backend: String,
    pub allow_writes: bool,
    pub tags: Vec<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCatalogResponse {
    pub service: String,
    pub items: Vec<McpCatalogItem>,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRuntimeSessionRequest {
    pub owner_user_id: String,
    pub agent_key: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub task_profile: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub default_model_config_id: Option<String>,
    #[serde(default)]
    pub expected_project_task_ids: Vec<String>,
    pub requested_device_id: Option<String>,
    pub requested_sandbox_provider: Option<SandboxProviderKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_target: Option<SandboxExecutionTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSessionResponse {
    pub session_id: String,
    pub policy_revision: String,
    pub route_revision: String,
    pub expires_at: String,
    pub mcp_server_url: String,
    pub runtime_token: String,
    pub configured_mcp_count: usize,
    #[serde(default)]
    pub exposed_tool_count: usize,
    #[serde(default)]
    pub unavailable_required_mcps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseRuntimeSessionResponse {
    pub session_id: String,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeToolDescriptor {
    pub exposed_name: String,
    pub original_name: String,
    pub resource_id: String,
    pub definition: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSessionRoutesResponse {
    pub session_id: String,
    pub owner_user_id: String,
    pub agent_key: String,
    pub project_id: String,
    pub policy_revision: String,
    pub route_revision: String,
    pub expires_at: String,
    pub routes: Vec<ResolvedMcpRoute>,
    pub tools: Vec<RuntimeToolDescriptor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_enums_use_wire_stable_snake_case_names() {
        assert_eq!(
            serde_json::to_string(&WorkspaceProviderKind::LocalConnector).unwrap(),
            "\"local_connector\""
        );
        assert_eq!(
            serde_json::to_string(&McpProviderKind::CloudSandbox).unwrap(),
            "\"cloud_sandbox\""
        );
    }

    #[test]
    fn exposed_tool_name_is_namespaced_by_server() {
        let route = ResolvedMcpRoute {
            resource_id: "builtin_code_maintainer_read".to_string(),
            server_name: "code_maintainer_read".to_string(),
            provider_kind: McpProviderKind::LocalConnector,
            provider_ref: Some("device:one/workspace:repo".to_string()),
            tool_namespace: "code_maintainer_read".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        };
        assert_eq!(
            route.exposed_tool_name("read_file"),
            "code_maintainer_read_read_file"
        );
    }

    #[test]
    fn sandbox_execution_target_provider_ref_contains_only_opaque_ids() {
        let target = SandboxExecutionTarget {
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        };
        assert_eq!(target.provider_ref(), "sandbox:sandbox-1/lease:lease-1");
    }
}
