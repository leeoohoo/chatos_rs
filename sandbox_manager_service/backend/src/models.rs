// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_sandbox_contract::{
    EffectivePermissionSnapshot, EffectiveSandboxPolicy, SandboxLeasePolicyRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Pending,
    Leasing,
    Starting,
    Ready,
    Running,
    Stopped,
    Releasing,
    Destroying,
    Destroyed,
    Failed,
    Expired,
}

impl SandboxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Leasing => "leasing",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Releasing => "releasing",
            Self::Destroying => "destroying",
            Self::Destroyed => "destroyed",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    pub fn is_active(self) -> bool {
        !matches!(self, Self::Destroyed | Self::Failed)
    }
}

impl std::str::FromStr for SandboxStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "leasing" => Ok(Self::Leasing),
            "starting" => Ok(Self::Starting),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "releasing" => Ok(Self::Releasing),
            "destroying" => Ok(Self::Destroying),
            "destroyed" => Ok(Self::Destroyed),
            "failed" => Ok(Self::Failed),
            "expired" => Ok(Self::Expired),
            _ => Err(format!("unknown sandbox status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu: f32,
    pub memory_mb: u64,
    pub disk_mb: u64,
    pub max_processes: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu: 2.0,
            memory_mb: 4096,
            disk_mb: 10240,
            max_processes: 512,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkPolicy {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxLeaseRecord {
    pub id: String,
    pub sandbox_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub project_id: String,
    pub run_id: String,
    pub workspace_root: String,
    pub run_workspace: String,
    pub backend: String,
    pub backend_id: Option<String>,
    #[serde(default)]
    pub image_id: Option<String>,
    #[serde(default)]
    pub image_ref: Option<String>,
    pub status: SandboxStatus,
    pub agent_endpoint: Option<String>,
    pub resource_limits: ResourceLimits,
    pub network: NetworkPolicy,
    pub tools: Vec<String>,
    #[serde(default = "default_single_lease_kind")]
    pub lease_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_service_id: Option<String>,
    #[serde(default)]
    pub environment_services: Vec<SandboxEnvironmentServiceRecord>,
    #[serde(default)]
    pub agent_token_nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub destroyed_at: Option<String>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub effective_policy: EffectiveSandboxPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_permissions: Option<EffectivePermissionSnapshot>,
}

fn default_single_lease_kind() -> String {
    "sandbox".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SandboxEnvironmentMcpPolicy {
    #[serde(default)]
    pub managed_by: String,
    #[serde(default)]
    pub attachment: String,
    #[serde(default)]
    pub filesystem: bool,
    #[serde(default)]
    pub terminal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEnvironmentServiceRecord {
    pub service_id: String,
    pub environment_key: String,
    pub display_name: String,
    pub service_role: String,
    pub image_id: Option<String>,
    pub image_ref: String,
    pub backend_id: Option<String>,
    pub status: String,
    pub agent_endpoint: Option<String>,
    #[serde(default)]
    pub mcp_policy: SandboxEnvironmentMcpPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSandboxEnvironmentLeaseRequest {
    pub tenant_id: String,
    pub user_id: String,
    pub project_id: String,
    pub run_id: String,
    pub workspace_root: String,
    pub ttl_seconds: Option<u64>,
    pub resource_limits: Option<ResourceLimits>,
    pub network: Option<NetworkPolicy>,
    #[serde(flatten)]
    pub policy: SandboxLeasePolicyRequest,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartSandboxEnvironmentRequest {
    pub lease_id: String,
    #[serde(default)]
    pub primary_service_id: Option<String>,
    #[serde(default)]
    pub services: Vec<SandboxEnvironmentServiceInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxEnvironmentServiceInput {
    pub service_id: String,
    pub environment_key: String,
    pub display_name: String,
    pub service_role: String,
    #[serde(default)]
    pub image_id: Option<String>,
    #[serde(default)]
    pub image_ref: Option<String>,
    #[serde(default)]
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub mcp_policy: SandboxEnvironmentMcpPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxEnvironmentLeaseResponse {
    pub lease_id: String,
    pub environment_id: String,
    pub backend_id: Option<String>,
    pub status: SandboxStatus,
    pub run_workspace: String,
    pub expires_at: String,
    pub primary_service_id: Option<String>,
    pub agent_endpoint: Option<String>,
    pub services: Vec<SandboxEnvironmentServiceRecord>,
    pub agent_token: String,
    pub effective_policy: EffectiveSandboxPolicy,
    pub effective_permissions: EffectivePermissionSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxEnvironmentStopRequest {
    pub lease_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxEnvironmentExecRequest {
    pub lease_id: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxEnvironmentExecResponse {
    pub service_id: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEventRecord {
    pub id: String,
    pub sandbox_id: String,
    pub lease_id: String,
    pub event_type: String,
    pub message: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSandboxLeaseRequest {
    pub tenant_id: String,
    pub user_id: String,
    pub project_id: String,
    pub run_id: String,
    pub workspace_root: String,
    pub image_id: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    pub ttl_seconds: Option<u64>,
    pub resource_limits: Option<ResourceLimits>,
    pub network: Option<NetworkPolicy>,
    #[serde(flatten)]
    pub policy: SandboxLeasePolicyRequest,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateSandboxLeaseResponse {
    pub lease_id: String,
    pub sandbox_id: String,
    pub backend_id: Option<String>,
    pub image_id: Option<String>,
    pub image_ref: Option<String>,
    pub status: SandboxStatus,
    pub agent_endpoint: Option<String>,
    pub agent_token: String,
    pub run_workspace: String,
    pub expires_at: String,
    pub effective_policy: EffectiveSandboxPolicy,
    pub effective_permissions: EffectivePermissionSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatRequest {
    pub lease_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeartbeatResponse {
    pub ok: bool,
    pub status: SandboxStatus,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxHealthCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxHealthResponse {
    pub ok: bool,
    pub sandbox_id: String,
    pub lease_id: String,
    pub status: SandboxStatus,
    pub backend: String,
    pub backend_id: Option<String>,
    pub backend_alive: bool,
    pub agent_endpoint: Option<String>,
    pub agent_alive: Option<bool>,
    pub workspace_alive: bool,
    pub checked_at: String,
    pub message: String,
    pub checks: Vec<SandboxHealthCheck>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseSandboxRequest {
    pub lease_id: String,
    #[serde(default)]
    pub export_result: bool,
    #[serde(default = "default_true")]
    pub destroy: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseSandboxResponse {
    pub ok: bool,
    pub status: SandboxStatus,
    pub output_workspace: Option<String>,
    pub diff_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_error: Option<String>,
    pub change_manifest: Option<SandboxOutputChangeManifest>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxOutputFileChangeCounts {
    #[serde(default)]
    pub added: usize,
    #[serde(default)]
    pub modified: usize,
    #[serde(default)]
    pub deleted: usize,
    #[serde(default)]
    pub binary: usize,
    #[serde(default)]
    pub diff_available: usize,
    #[serde(default)]
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxOutputFileChange {
    pub path: String,
    pub status: String,
    #[serde(default)]
    pub old_size: Option<u64>,
    #[serde(default)]
    pub new_size: Option<u64>,
    #[serde(default)]
    pub old_sha256: Option<String>,
    #[serde(default)]
    pub new_sha256: Option<String>,
    #[serde(default)]
    pub added_lines: usize,
    #[serde(default)]
    pub deleted_lines: usize,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub diff_available: bool,
    #[serde(default)]
    pub diff_truncated: bool,
    #[serde(default)]
    pub diff_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxOutputChangeManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub sandbox_id: String,
    pub lease_id: String,
    pub generated_at: String,
    #[serde(default)]
    pub output_workspace: Option<String>,
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub counts: SandboxOutputFileChangeCounts,
    #[serde(default)]
    pub files: Vec<SandboxOutputFileChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DestroySandboxResponse {
    pub ok: bool,
    pub status: SandboxStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListSandboxQuery {
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxAccessClientRecord {
    pub id: String,
    pub name: String,
    pub client_id: String,
    pub key_hash: String,
    pub enabled: bool,
    pub scopes: Vec<String>,
    pub allowed_tenant_ids: Vec<String>,
    pub allowed_project_ids: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub max_lease_ttl_seconds: u64,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxAccessClientResponse {
    pub id: String,
    pub name: String,
    pub client_id: String,
    pub enabled: bool,
    pub scopes: Vec<String>,
    pub allowed_tenant_ids: Vec<String>,
    pub allowed_project_ids: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub max_lease_ttl_seconds: u64,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateSandboxAccessClientResponse {
    pub client: SandboxAccessClientResponse,
    pub client_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RotateSandboxAccessClientKeyResponse {
    pub client: SandboxAccessClientResponse,
    pub client_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSandboxAccessClientRequest {
    pub name: String,
    pub client_id: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub allowed_tenant_ids: Vec<String>,
    #[serde(default)]
    pub allowed_project_ids: Vec<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub max_lease_ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSandboxAccessClientRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub scopes: Option<Vec<String>>,
    pub allowed_tenant_ids: Option<Vec<String>>,
    pub allowed_project_ids: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_lease_ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteSandboxAccessClientResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolStatusResponse {
    pub backend: String,
    pub max_active: usize,
    pub active: usize,
    pub max_pending: usize,
    pub pending: usize,
    pub lease_ttl_seconds: u64,
    pub cleanup_interval_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePoolConfigRequest {
    pub max_active: Option<usize>,
    pub max_pending: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemConfigResponse {
    pub host: String,
    pub port: u16,
    pub backend: String,
    pub work_root: String,
    pub pool_max_active: usize,
    pub pool_max_pending: usize,
    pub lease_ttl_seconds: u64,
    pub cleanup_interval_seconds: u64,
    pub agent_port: u16,
    pub docker_image: String,
    pub docker_network_mode: String,
    pub kata_container_cli: String,
    pub kata_runtime: String,
    pub kata_image: String,
    pub kata_network_mode: String,
    pub image_tag_prefix: String,
    pub image_build_context: String,
    pub image_dockerfile: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxImageRuntimeVersionRecord {
    pub id: String,
    pub label: String,
    pub description: String,
    pub default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxImageFeatureRecord {
    pub id: String,
    pub label: String,
    pub description: String,
    pub default_version: String,
    pub versions: Vec<SandboxImageRuntimeVersionRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxImageRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub image_ref: String,
    pub features: Vec<String>,
    pub backend: String,
    pub initialized: bool,
    pub status: String,
    pub buildable: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxImageCatalogResponse {
    pub backend: String,
    pub default_image_id: String,
    pub image_tag_prefix: String,
    pub features: Vec<SandboxImageFeatureRecord>,
    pub images: Vec<SandboxImageRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InitializeSandboxImageRequest {
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub custom_build_script: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareSandboxDependencyImagesRequest {
    #[serde(default)]
    pub image_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreparedSandboxDependencyImageRecord {
    pub image_ref: String,
    pub reused: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareSandboxDependencyImagesResponse {
    pub images: Vec<PreparedSandboxDependencyImageRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxImageJobRecord {
    pub id: String,
    pub image_id: String,
    pub image_name: String,
    pub image_ref: String,
    pub features: Vec<String>,
    pub backend: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub output: String,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::ResourceLimits;

    #[test]
    fn default_resource_limits_support_browser_e2e_processes() {
        let limits = ResourceLimits::default();

        assert_eq!(limits.max_processes, 512);
        assert!(limits.memory_mb >= 4096);
    }
}
