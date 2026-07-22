// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::process::Command;

use crate::config::{AppConfig, SandboxBackendKind};
use crate::models::{NetworkPolicy, ResourceLimits};

mod docker;
mod kata;
mod mock;

pub use docker::DockerSandboxBackend;
pub use kata::KataSandboxBackend;
pub use mock::MockSandboxBackend;

pub type SandboxBackendRef = Arc<dyn SandboxBackend>;

#[derive(Debug, Clone)]
pub struct SandboxCreateSpec {
    pub sandbox_id: String,
    pub run_workspace: String,
    pub image: String,
    pub agent_token: Option<String>,
    pub resource_limits: ResourceLimits,
    pub network: NetworkPolicy,
}

#[derive(Debug, Clone)]
pub struct SandboxInstance {
    pub sandbox_id: String,
    pub backend_id: Option<String>,
    pub agent_endpoint: Option<String>,
}

fn append_sandbox_create_runtime_args(
    command: &mut Command,
    spec: &SandboxCreateSpec,
    network: &str,
    cpu: &str,
    memory: &str,
    pids: &str,
    disk_limit_bytes: u64,
) {
    command
        .arg("--network")
        .arg(network)
        .arg("--cpus")
        .arg(cpu)
        .arg("--memory")
        .arg(memory)
        .arg("--pids-limit")
        .arg(pids)
        .arg("--workdir")
        .arg("/workspace")
        .arg("-e")
        .arg(format!("CHATOS_SANDBOX_ID={}", spec.sandbox_id))
        .arg("-e")
        .arg("CHATOS_SANDBOX_PERMISSION_PROFILE=workspace_write")
        .arg("-e")
        .arg(format!(
            "CHATOS_SANDBOX_DISK_LIMIT_BYTES={disk_limit_bytes}"
        ))
        .arg("-e")
        .arg("HOME=/home/sandbox")
        .arg("-e")
        .arg("XDG_CACHE_HOME=/home/sandbox/.cache");
    if let Some(agent_token) = spec.agent_token.as_deref() {
        command
            .arg("-e")
            .arg(format!("CHATOS_SANDBOX_MCP_TOKEN={agent_token}"));
    }
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentCreateSpec {
    pub environment_id: String,
    pub run_workspace: String,
    pub services: Vec<SandboxEnvironmentServiceSpec>,
    pub agent_token: String,
    pub resource_limits: ResourceLimits,
    pub network: NetworkPolicy,
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentServiceSpec {
    pub service_id: String,
    pub service_role: String,
    pub image: String,
    pub dockerfile: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub mcp_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentServiceInstance {
    pub service_id: String,
    pub backend_id: Option<String>,
    pub status: String,
    pub agent_endpoint: Option<String>,
    pub image_ref: String,
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentInstance {
    pub environment_id: String,
    pub backend_id: Option<String>,
    pub services: Vec<SandboxEnvironmentServiceInstance>,
}

#[derive(Debug, Clone)]
pub struct SandboxExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait SandboxBackend: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String>;
    async fn start(&self, sandbox_id: &str) -> Result<(), String>;
    async fn stop(&self, sandbox_id: &str) -> Result<(), String>;
    async fn destroy(&self, sandbox_id: &str, backend_id: Option<&str>) -> Result<(), String>;
    async fn inspect(
        &self,
        sandbox_id: &str,
        backend_id: Option<&str>,
    ) -> Result<Option<SandboxInstance>, String>;
    async fn create_environment(
        &self,
        _spec: SandboxEnvironmentCreateSpec,
    ) -> Result<SandboxEnvironmentInstance, String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn start_environment(&self, _environment_id: &str) -> Result<(), String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn stop_environment(&self, _environment_id: &str) -> Result<(), String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn destroy_environment(&self, _environment_id: &str) -> Result<(), String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn inspect_environment(
        &self,
        _environment_id: &str,
    ) -> Result<Option<SandboxEnvironmentInstance>, String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn exec_environment_service(
        &self,
        _environment_id: &str,
        _service_id: &str,
        _command: &[String],
    ) -> Result<SandboxExecResult, String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
}

pub fn build_backend(config: &AppConfig) -> SandboxBackendRef {
    match config.backend {
        SandboxBackendKind::Docker => Arc::new(DockerSandboxBackend::new(config.clone())),
        SandboxBackendKind::Kata => Arc::new(KataSandboxBackend::new(config.clone())),
        SandboxBackendKind::Mock => Arc::new(MockSandboxBackend::default()),
    }
}
