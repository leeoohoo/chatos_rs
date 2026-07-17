// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::{
    SandboxBackend, SandboxCreateSpec, SandboxEnvironmentCreateSpec, SandboxEnvironmentInstance,
    SandboxEnvironmentServiceInstance, SandboxExecResult, SandboxInstance,
};

#[derive(Debug, Default)]
pub struct MockSandboxBackend {
    instances: Arc<RwLock<HashMap<String, SandboxInstance>>>,
    environments: Arc<RwLock<HashMap<String, SandboxEnvironmentInstance>>>,
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

    async fn create_environment(
        &self,
        spec: SandboxEnvironmentCreateSpec,
    ) -> Result<SandboxEnvironmentInstance, String> {
        let instance = SandboxEnvironmentInstance {
            environment_id: spec.environment_id.clone(),
            backend_id: Some(format!("mock-environment-{}", spec.environment_id)),
            services: spec
                .services
                .into_iter()
                .map(|service| SandboxEnvironmentServiceInstance {
                    service_id: service.service_id.clone(),
                    backend_id: Some(format!(
                        "mock-environment-{}-{}",
                        spec.environment_id, service.service_id
                    )),
                    status: "running".to_string(),
                    agent_endpoint: service
                        .mcp_enabled
                        .then(|| format!("mock://{}/{}", spec.environment_id, service.service_id)),
                    image_ref: service.image,
                })
                .collect(),
        };
        self.environments
            .write()
            .await
            .insert(spec.environment_id, instance.clone());
        Ok(instance)
    }

    async fn stop_environment(&self, environment_id: &str) -> Result<(), String> {
        if let Some(environment) = self.environments.write().await.get_mut(environment_id) {
            for service in &mut environment.services {
                service.status = "stopped".to_string();
            }
        }
        Ok(())
    }

    async fn start_environment(&self, environment_id: &str) -> Result<(), String> {
        let mut environments = self.environments.write().await;
        let environment = environments
            .get_mut(environment_id)
            .ok_or_else(|| format!("mock environment not found: {environment_id}"))?;
        for service in &mut environment.services {
            service.status = "running".to_string();
        }
        Ok(())
    }

    async fn destroy_environment(&self, environment_id: &str) -> Result<(), String> {
        self.environments.write().await.remove(environment_id);
        Ok(())
    }

    async fn inspect_environment(
        &self,
        environment_id: &str,
    ) -> Result<Option<SandboxEnvironmentInstance>, String> {
        Ok(self.environments.read().await.get(environment_id).cloned())
    }

    async fn exec_environment_service(
        &self,
        environment_id: &str,
        service_id: &str,
        command: &[String],
    ) -> Result<SandboxExecResult, String> {
        let exists = self
            .environments
            .read()
            .await
            .get(environment_id)
            .is_some_and(|environment| {
                environment
                    .services
                    .iter()
                    .any(|service| service.service_id == service_id)
            });
        if !exists {
            return Err(format!(
                "mock environment service not found: {environment_id}/{service_id}"
            ));
        }
        Ok(SandboxExecResult {
            exit_code: 0,
            stdout: command.join(" "),
            stderr: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{SandboxEnvironmentServiceSpec, SandboxExecResult};
    use crate::models::{NetworkPolicy, ResourceLimits};
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn environment_lifecycle_keeps_dependencies_agent_free() {
        let backend = MockSandboxBackend::default();
        let environment = backend
            .create_environment(SandboxEnvironmentCreateSpec {
                environment_id: "environment-1".to_string(),
                run_workspace: "/workspace".to_string(),
                services: vec![
                    SandboxEnvironmentServiceSpec {
                        service_id: "redis".to_string(),
                        service_role: "dependency".to_string(),
                        image: "redis:7-alpine".to_string(),
                        dockerfile: None,
                        environment: BTreeMap::new(),
                        mcp_enabled: false,
                    },
                    SandboxEnvironmentServiceSpec {
                        service_id: "api".to_string(),
                        service_role: "application".to_string(),
                        image: "chatos-sandbox-agent:latest".to_string(),
                        dockerfile: Some("FROM alpine\n".to_string()),
                        environment: BTreeMap::new(),
                        mcp_enabled: true,
                    },
                ],
                agent_token: "program-token".to_string(),
                resource_limits: ResourceLimits::default(),
                network: NetworkPolicy::default(),
            })
            .await
            .expect("create environment");

        let dependency = environment
            .services
            .iter()
            .find(|service| service.service_id == "redis")
            .expect("dependency");
        assert!(dependency.agent_endpoint.is_none());
        let application = environment
            .services
            .iter()
            .find(|service| service.service_id == "api")
            .expect("application");
        assert!(application.agent_endpoint.is_some());

        backend
            .stop_environment("environment-1")
            .await
            .expect("stop");
        assert!(backend
            .inspect_environment("environment-1")
            .await
            .expect("inspect")
            .expect("environment")
            .services
            .iter()
            .all(|service| service.status == "stopped"));
        backend
            .start_environment("environment-1")
            .await
            .expect("restart");
        let SandboxExecResult { stdout, .. } = backend
            .exec_environment_service(
                "environment-1",
                "api",
                &["echo".to_string(), "ok".to_string()],
            )
            .await
            .expect("exec application");
        assert_eq!(stdout, "echo ok");
        backend
            .destroy_environment("environment-1")
            .await
            .expect("destroy");
        assert!(backend
            .inspect_environment("environment-1")
            .await
            .expect("inspect destroyed")
            .is_none());
    }
}
