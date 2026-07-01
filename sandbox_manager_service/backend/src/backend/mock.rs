// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::{SandboxBackend, SandboxCreateSpec, SandboxInstance};

#[derive(Debug, Default)]
pub struct MockSandboxBackend {
    instances: Arc<RwLock<HashMap<String, SandboxInstance>>>,
}

#[async_trait]
impl SandboxBackend for MockSandboxBackend {
    fn kind(&self) -> &'static str {
        "mock"
    }

    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String> {
        let instance = SandboxInstance {
            sandbox_id: spec.sandbox_id.clone(),
            backend_id: Some(format!("mock-{}", spec.sandbox_id)),
            agent_endpoint: Some(format!("mock://{}", spec.sandbox_id)),
        };
        self.instances
            .write()
            .await
            .insert(spec.sandbox_id, instance.clone());
        Ok(instance)
    }

    async fn start(&self, _sandbox_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn stop(&self, _sandbox_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn destroy(&self, sandbox_id: &str, _backend_id: Option<&str>) -> Result<(), String> {
        self.instances.write().await.remove(sandbox_id);
        Ok(())
    }

    async fn inspect(
        &self,
        sandbox_id: &str,
        _backend_id: Option<&str>,
    ) -> Result<Option<SandboxInstance>, String> {
        Ok(self.instances.read().await.get(sandbox_id).cloned())
    }
}
