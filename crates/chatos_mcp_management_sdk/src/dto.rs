// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceProviderKind {
    LocalConnector,
    #[default]
    None,
}

impl WorkspaceProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalConnector => "local_connector",
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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeWorkspaceRouteTarget {
    LocalConnector {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_tool_root: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        owned_paths: Vec<String>,
    },
}

impl RuntimeWorkspaceRouteTarget {
    pub const fn provider_kind(&self) -> WorkspaceProviderKind {
        match self {
            Self::LocalConnector { .. } => WorkspaceProviderKind::LocalConnector,
        }
    }

    pub fn local_connector_default_tool_root(&self) -> Option<&str> {
        match self {
            Self::LocalConnector {
                default_tool_root, ..
            } => default_tool_root.as_deref(),
        }
    }

    pub fn local_connector_owned_paths(&self) -> &[String] {
        match self {
            Self::LocalConnector { owned_paths, .. } => owned_paths.as_slice(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectExecutionContext {
    pub project_id: String,
    pub owner_user_id: String,
    #[serde(default)]
    pub workspace_provider: WorkspaceProviderKind,
    pub workspace: Option<WorkspaceExecutionTarget>,
    pub revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpProviderKind {
    Embedded,
    InternalService,
    LocalConnector,
    ExternalHttp,
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
            Self::ExternalHttp => "external_http",
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
    pub tenant_id: String,
    pub owner_user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_role: Option<String>,
    pub agent_key: String,
    pub project_id: String,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_group_id: Option<String>,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub task_profile: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_agent_id: Option<String>,
    pub default_model_config_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result_max_chars: Option<usize>,
    #[serde(default)]
    pub expected_project_task_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_mcp_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_plugins: Vec<chatos_plugin_management_sdk::SelectedPluginRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_command_invocations: Vec<chatos_plugin_management_sdk::PluginCommandInvocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_route: Option<RuntimeWorkspaceRouteTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProviderFinalizationStatus {
    Succeeded,
    NoChanges,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProviderChangedFile {
    pub status: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProviderFinalization {
    pub provider_kind: McpProviderKind,
    pub status: RuntimeProviderFinalizationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_branch_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrated_commit: Option<String>,
    #[serde(default)]
    pub conflict_files: Vec<String>,
    #[serde(default)]
    pub files: Vec<RuntimeProviderChangedFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    #[serde(default)]
    pub patch_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSessionResponse {
    pub session_id: String,
    pub policy_revision: String,
    pub route_revision: String,
    pub expires_at: String,
    pub mcp_server_url: String,
    pub mcp_command_queue: String,
    pub runtime_token: String,
    pub configured_mcp_count: usize,
    #[serde(default)]
    pub exposed_tool_count: usize,
    #[serde(default)]
    pub effective_mcp_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_skills_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_instruction_items: Vec<serde_json::Value>,
    #[serde(default)]
    pub unavailable_required_mcps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseRuntimeSessionResponse {
    pub session_id: String,
    pub closed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_finalization: Option<RuntimeProviderFinalization>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInvocationStatus {
    Queued,
    Running,
    WaitingForUser,
    CancelRequested,
    Completed,
    Failed,
    Cancelled,
    UnknownExecutionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInvocationResponse {
    pub invocation_id: String,
    pub session_id: String,
    pub caller_service: String,
    pub resource_id: String,
    pub exposed_tool_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_tool_name: Option<String>,
    pub status: RuntimeInvocationStatus,
    pub created_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_error_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_modification_outcome: Option<String>,
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
    pub tenant_id: String,
    pub owner_user_id: String,
    pub agent_key: String,
    pub project_id: String,
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_route: Option<RuntimeWorkspaceRouteTarget>,
    pub policy_revision: String,
    pub route_revision: String,
    pub expires_at: String,
    pub routes: Vec<ResolvedMcpRoute>,
    pub tools: Vec<RuntimeToolDescriptor>,
    #[serde(default)]
    pub mcp_command_queue: String,
    #[serde(default)]
    pub mcp_server_url: String,
    #[serde(default)]
    pub runtime_token: String,
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
            serde_json::to_string(&McpProviderKind::LocalConnector).unwrap(),
            "\"local_connector\""
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
    fn runtime_session_response_requires_event_queue_and_defaults_optional_prompt_metadata() {
        let incomplete = serde_json::json!({
            "session_id": "session-1",
            "policy_revision": "policy-1",
            "route_revision": "route-1",
            "expires_at": "2099-01-01T00:00:00Z",
            "mcp_server_url": "http://mcp-management/mcp",
            "runtime_token": "token",
            "configured_mcp_count": 1
        });
        assert!(serde_json::from_value::<RuntimeSessionResponse>(incomplete.clone()).is_err());
        let mut complete = incomplete;
        complete["mcp_command_queue"] = serde_json::json!("mcp.commands");
        let response: RuntimeSessionResponse =
            serde_json::from_value(complete).expect("current runtime session response");
        assert!(response.effective_mcp_ids.is_empty());
        assert!(response.provider_skills_prompt.is_none());
        assert!(response.plugin_instruction_items.is_empty());
    }
}
