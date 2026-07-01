// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use tokio::process::Command;

use crate::config::AppConfig;

use super::{SandboxBackend, SandboxCreateSpec, SandboxInstance};

#[derive(Debug, Clone)]
pub struct DockerSandboxBackend {
    config: AppConfig,
}

impl DockerSandboxBackend {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SandboxBackend for DockerSandboxBackend {
    fn kind(&self) -> &'static str {
        "docker"
    }

    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String> {
        let name = docker_name(spec.sandbox_id.as_str());
        let cpu = spec.resource_limits.cpu.max(0.1).to_string();
        let memory = format!("{}m", spec.resource_limits.memory_mb.max(128));
        let pids = spec.resource_limits.max_processes.max(16).to_string();
        let requested_network = spec.network.mode.trim();
        let network = if requested_network.is_empty() {
            self.config.docker_network_mode.as_str()
        } else {
            requested_network
        };
        let publish_agent = network != "none" && self.config.agent_port > 0;
        let mut command = Command::new("docker");
        command
            .arg("run")
            .arg("-d")
            .arg("--name")
            .arg(&name)
            .arg("--hostname")
            .arg(&name)
            .arg("--label")
            .arg(format!("chatos.sandbox_id={}", spec.sandbox_id))
            .arg("--label")
            .arg("chatos.backend=docker")
            .arg("--network")
            .arg(network)
            .arg("--cpus")
            .arg(cpu)
            .arg("--memory")
            .arg(memory)
            .arg("--pids-limit")
            .arg(pids)
            .arg("--workdir")
            .arg("/workspace");
        if publish_agent {
            command
                .arg("-p")
                .arg(format!("127.0.0.1::{}", self.config.agent_port));
        }
        command
            .arg("--tmpfs")
            .arg("/tmp:rw,nosuid,size=512m")
            .arg("--security-opt")
            .arg("no-new-privileges")
            .arg("-v")
            .arg(format!("{}:/workspace:rw", spec.run_workspace))
            .arg(&self.config.docker_image);
        let output = command.output().await.map_err(|err| {
            format!("start docker sandbox failed: {err}; is docker installed and running?")
        })?;
        if !output.status.success() {
            return Err(format!(
                "docker run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(SandboxInstance {
            sandbox_id: spec.sandbox_id,
            backend_id: Some(container_id),
            agent_endpoint: if publish_agent {
                published_agent_endpoint("docker", &name, self.config.agent_port).await
            } else {
                None
            },
        })
    }

    async fn start(&self, _sandbox_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn stop(&self, sandbox_id: &str) -> Result<(), String> {
        let name = docker_name(sandbox_id);
        let _ = Command::new("docker")
            .arg("stop")
            .arg(&name)
            .output()
            .await
            .map_err(|err| format!("docker stop failed: {err}"))?;
        Ok(())
    }

    async fn destroy(&self, sandbox_id: &str, _backend_id: Option<&str>) -> Result<(), String> {
        let name = docker_name(sandbox_id);
        let output = Command::new("docker")
            .arg("rm")
            .arg("-f")
            .arg(&name)
            .output()
            .await
            .map_err(|err| format!("docker rm failed: {err}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("No such container") {
                return Err(format!("docker rm failed: {stderr}"));
            }
        }
        Ok(())
    }

    async fn inspect(
        &self,
        sandbox_id: &str,
        _backend_id: Option<&str>,
    ) -> Result<Option<SandboxInstance>, String> {
        let name = docker_name(sandbox_id);
        let output = Command::new("docker")
            .arg("inspect")
            .arg(&name)
            .output()
            .await
            .map_err(|err| format!("docker inspect failed: {err}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let agent_endpoint =
            published_agent_endpoint("docker", &name, self.config.agent_port).await;
        Ok(Some(SandboxInstance {
            sandbox_id: sandbox_id.to_string(),
            backend_id: Some(name),
            agent_endpoint,
        }))
    }
}

fn docker_name(sandbox_id: &str) -> String {
    format!("chatos-sandbox-{sandbox_id}")
}

async fn published_agent_endpoint(cli: &str, name: &str, agent_port: u16) -> Option<String> {
    let output = Command::new(cli)
        .arg("port")
        .arg(name)
        .arg(format!("{agent_port}/tcp"))
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim();
    let host_port = line.rsplit(':').next()?.trim();
    if host_port.is_empty() {
        return None;
    }
    Some(format!("http://127.0.0.1:{host_port}"))
}
