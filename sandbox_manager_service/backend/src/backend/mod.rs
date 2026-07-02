// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;

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
}

pub fn build_backend(config: &AppConfig) -> SandboxBackendRef {
    match config.backend {
        SandboxBackendKind::Docker => Arc::new(DockerSandboxBackend::new(config.clone())),
        SandboxBackendKind::Kata => Arc::new(KataSandboxBackend::new(config.clone())),
        SandboxBackendKind::Mock => Arc::new(MockSandboxBackend::default()),
    }
}
