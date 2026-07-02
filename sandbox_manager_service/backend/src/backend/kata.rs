// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use tokio::process::Command;

use crate::config::AppConfig;

use super::{SandboxBackend, SandboxCreateSpec, SandboxInstance};

#[derive(Debug, Clone)]
pub struct KataSandboxBackend {
    config: AppConfig,
}

impl KataSandboxBackend {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl SandboxBackend for KataSandboxBackend {
    fn kind(&self) -> &'static str {
        "kata"
    }

    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String> {
        let name = kata_name(spec.sandbox_id.as_str());
        let cpu = spec.resource_limits.cpu.max(0.1).to_string();
        let memory = format!("{}m", spec.resource_limits.memory_mb.max(128));
        let pids = spec.resource_limits.max_processes.max(16).to_string();
        let requested_network = spec.network.mode.trim();
        let network = if requested_network.is_empty() {
            self.config.kata_network_mode.as_str()
        } else {
            requested_network
        };
        let publish_agent = network != "none" && self.config.agent_port > 0;

        let mut command = Command::new(&self.config.kata_container_cli);
        command
            .arg("run")
            .arg("-d")
            .arg("--name")
            .arg(&name)
            .arg("--hostname")
            .arg(&name)
            .arg("--runtime")
            .arg(&self.config.kata_runtime)
            .arg("--label")
            .arg(format!("chatos.sandbox_id={}", spec.sandbox_id))
            .arg("--label")
            .arg("chatos.backend=kata")
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
        command
            .arg("-e")
            .arg(format!("CHATOS_SANDBOX_ID={}", spec.sandbox_id));
        if let Some(agent_token) = spec.agent_token.as_deref() {
            command
                .arg("-e")
                .arg(format!("CHATOS_SANDBOX_MCP_TOKEN={agent_token}"));
        }
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
            .arg(&spec.image);

        let output = command.output().await.map_err(|err| {
            format!(
                "start kata sandbox failed: {err}; is {} installed and is Kata configured?",
                self.config.kata_container_cli
            )
        })?;
        if !output.status.success() {
            return Err(format!(
                "kata run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(SandboxInstance {
            sandbox_id: spec.sandbox_id,
            backend_id: Some(container_id),
            agent_endpoint: if publish_agent {
                published_agent_endpoint(
                    &self.config.kata_container_cli,
                    &name,
                    self.config.agent_port,
                )
                .await
            } else {
                None
            },
        })
    }

    async fn start(&self, _sandbox_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn stop(&self, sandbox_id: &str) -> Result<(), String> {
        let name = kata_name(sandbox_id);
        let _ = Command::new(&self.config.kata_container_cli)
            .arg("stop")
            .arg(&name)
            .output()
            .await
            .map_err(|err| format!("kata stop failed: {err}"))?;
        Ok(())
    }

    async fn destroy(&self, sandbox_id: &str, _backend_id: Option<&str>) -> Result<(), String> {
        let name = kata_name(sandbox_id);
        let output = Command::new(&self.config.kata_container_cli)
            .arg("rm")
            .arg("-f")
            .arg(&name)
            .output()
            .await
            .map_err(|err| format!("kata rm failed: {err}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !is_missing_container_message(&stderr) {
                return Err(format!("kata rm failed: {stderr}"));
            }
        }
        Ok(())
    }

    async fn inspect(
        &self,
        sandbox_id: &str,
        _backend_id: Option<&str>,
    ) -> Result<Option<SandboxInstance>, String> {
        let name = kata_name(sandbox_id);
        let output = Command::new(&self.config.kata_container_cli)
            .arg("inspect")
            .arg(&name)
            .output()
            .await
            .map_err(|err| format!("kata inspect failed: {err}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let agent_endpoint = published_agent_endpoint(
            &self.config.kata_container_cli,
            &name,
            self.config.agent_port,
        )
        .await;
        Ok(Some(SandboxInstance {
            sandbox_id: sandbox_id.to_string(),
            backend_id: Some(name),
            agent_endpoint,
        }))
    }
}

fn kata_name(sandbox_id: &str) -> String {
    format!("chatos-sandbox-{sandbox_id}")
}

fn is_missing_container_message(stderr: &str) -> bool {
    stderr.contains("No such container")
        || stderr.contains("not found")
        || stderr.contains("No such object")
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
