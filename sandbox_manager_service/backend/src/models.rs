// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Pending,
    Leasing,
    Starting,
    Ready,
    Running,
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
            max_processes: 128,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    pub mode: String,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            mode: String::new(),
        }
    }
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
    pub status: SandboxStatus,
    pub agent_endpoint: Option<String>,
    pub resource_limits: ResourceLimits,
    pub network: NetworkPolicy,
    pub tools: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub destroyed_at: Option<String>,
    pub last_error: Option<String>,
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
    #[serde(default)]
    pub tools: Vec<String>,
    pub ttl_seconds: Option<u64>,
    pub resource_limits: Option<ResourceLimits>,
    pub network: Option<NetworkPolicy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateSandboxLeaseResponse {
    pub lease_id: String,
    pub sandbox_id: String,
    pub backend_id: Option<String>,
    pub status: SandboxStatus,
    pub agent_endpoint: Option<String>,
    pub run_workspace: String,
    pub expires_at: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct SandboxMcpToolsResponse {
    pub ok: bool,
    pub sandbox_id: String,
    pub agent_endpoint: String,
    pub tools: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxMcpCallRequest {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxMcpCallResponse {
    pub ok: bool,
    pub sandbox_id: String,
    pub agent_endpoint: String,
    pub result: Value,
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
}

fn default_true() -> bool {
    true
}
