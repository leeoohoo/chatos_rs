// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::process::Command;

use crate::config::{AppConfig, DockerAgentEndpointMode};

use super::{SandboxBackend, SandboxCreateSpec, SandboxInstance};

#[derive(Debug, Clone)]
pub struct DockerSandboxBackend {
    config: AppConfig,
    api: Option<DockerApiClient>,
}

#[derive(Debug, Clone)]
struct DockerApiClient {
    client: Client,
    base_url: String,
}

impl DockerSandboxBackend {
    pub fn new(config: AppConfig) -> Self {
        let api =
            docker_api_base_url(std::env::var("DOCKER_HOST").ok().as_deref()).map(|base_url| {
                DockerApiClient {
                    client: Client::new(),
                    base_url,
                }
            });
        Self { config, api }
    }
}

#[async_trait]
impl SandboxBackend for DockerSandboxBackend {
    fn kind(&self) -> &'static str {
        "docker"
    }

    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String> {
        if self.api.is_some() {
            return self.create_with_api(spec).await;
        }
        self.create_with_cli(spec).await
    }

    async fn start(&self, _sandbox_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn stop(&self, sandbox_id: &str) -> Result<(), String> {
        if self.api.is_some() {
            return self.stop_with_api(sandbox_id).await;
        }
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
        if self.api.is_some() {
            return self.destroy_with_api(sandbox_id).await;
        }
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
        if self.api.is_some() {
            return self.inspect_with_api(sandbox_id).await;
        }
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
        let agent_endpoint = self
            .agent_endpoint(&name, self.config.docker_agent_publish)
            .await;
        Ok(Some(SandboxInstance {
            sandbox_id: sandbox_id.to_string(),
            backend_id: Some(name),
            agent_endpoint,
        }))
    }
}

impl DockerSandboxBackend {
    async fn create_with_cli(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String> {
        let name = docker_name(spec.sandbox_id.as_str());
        let cpu = spec.resource_limits.cpu.max(0.1).to_string();
        let memory = format!("{}m", spec.resource_limits.memory_mb.max(128));
        let pids = spec.resource_limits.max_processes.max(16).to_string();
        let tmpfs_size_mb = (spec.resource_limits.disk_mb / 16).clamp(16, 512);
        let workspace_limit_mb = spec
            .resource_limits
            .disk_mb
            .saturating_sub(tmpfs_size_mb.saturating_mul(2))
            .max(1);
        let disk_limit_bytes = workspace_limit_mb.saturating_mul(1024 * 1024);
        let requested_network = spec.network.mode.trim();
        let network = if requested_network.is_empty() {
            self.config.docker_network_mode.as_str()
        } else {
            requested_network
        };
        let publish_agent =
            self.config.docker_agent_publish && network != "none" && self.config.agent_port > 0;
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
        command
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
        if publish_agent {
            command.arg("-p").arg(format!(
                "{}::{}",
                self.config.docker_agent_bind_host, self.config.agent_port
            ));
        }
        command
            .arg("--read-only")
            .arg("--cap-drop")
            .arg("ALL")
            .arg("--user")
            .arg("1000:1000")
            .arg("--tmpfs")
            .arg(format!(
                "/tmp:rw,nosuid,nodev,size={tmpfs_size_mb}m,mode=1777"
            ))
            .arg("--tmpfs")
            .arg(format!(
                "/home/sandbox:rw,nosuid,nodev,size={tmpfs_size_mb}m,uid=1000,gid=1000,mode=0700"
            ))
            .arg("--security-opt")
            .arg("no-new-privileges")
            .arg("-v")
            .arg(format!("{}:/workspace:rw", spec.run_workspace))
            .arg(&spec.image);
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
            agent_endpoint: self.agent_endpoint(&name, publish_agent).await,
        })
    }

    async fn create_with_api(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String> {
        let api = self
            .api
            .as_ref()
            .ok_or_else(|| "Docker API is not configured".to_string())?;
        let name = docker_name(spec.sandbox_id.as_str());
        let requested_network = spec.network.mode.trim();
        let network = if requested_network.is_empty() {
            self.config.docker_network_mode.as_str()
        } else {
            requested_network
        };
        let publish_agent =
            self.config.docker_agent_publish && network != "none" && self.config.agent_port > 0;
        let tmpfs_size_mb = (spec.resource_limits.disk_mb / 16).clamp(16, 512);
        let workspace_limit_mb = spec
            .resource_limits
            .disk_mb
            .saturating_sub(tmpfs_size_mb.saturating_mul(2))
            .max(1);
        let disk_limit_bytes = workspace_limit_mb.saturating_mul(1024 * 1024);
        let mut env = vec![
            format!("CHATOS_SANDBOX_ID={}", spec.sandbox_id),
            "CHATOS_SANDBOX_PERMISSION_PROFILE=workspace_write".to_string(),
            format!("CHATOS_SANDBOX_DISK_LIMIT_BYTES={disk_limit_bytes}"),
            "HOME=/home/sandbox".to_string(),
            "XDG_CACHE_HOME=/home/sandbox/.cache".to_string(),
        ];
        if let Some(agent_token) = spec.agent_token.as_deref() {
            env.push(format!("CHATOS_SANDBOX_MCP_TOKEN={agent_token}"));
        }
        let labels = HashMap::from([
            ("chatos.sandbox_id".to_string(), spec.sandbox_id.clone()),
            ("chatos.backend".to_string(), "docker".to_string()),
        ]);
        let port_key = format!("{}/tcp", self.config.agent_port);
        let mut payload = json!({
            "Image": spec.image,
            "Hostname": name,
            "WorkingDir": "/workspace",
            "User": "1000:1000",
            "Env": env,
            "Labels": labels,
            "HostConfig": {
                "NetworkMode": network,
                "NanoCpus": (spec.resource_limits.cpu.max(0.1) * 1_000_000_000_f32).round() as i64,
                "Memory": spec.resource_limits.memory_mb.max(128) as i64 * 1024 * 1024,
                "PidsLimit": spec.resource_limits.max_processes.max(16),
                "ReadonlyRootfs": true,
                "CapDrop": ["ALL"],
                "Tmpfs": {
                    "/tmp": format!("rw,nosuid,nodev,size={tmpfs_size_mb}m,mode=1777"),
                    "/home/sandbox": format!("rw,nosuid,nodev,size={tmpfs_size_mb}m,uid=1000,gid=1000,mode=0700")
                },
                "SecurityOpt": ["no-new-privileges"],
                "Binds": [format!("{}:/workspace:rw", spec.run_workspace)],
            }
        });
        if publish_agent {
            payload["ExposedPorts"] = json!({port_key.clone(): {}});
            payload["HostConfig"]["PortBindings"] = json!({
                port_key: [{
                    "HostIp": self.config.docker_agent_bind_host,
                    "HostPort": ""
                }]
            });
        }

        let response = api
            .client
            .post(api.url("/containers/create"))
            .query(&[("name", name.as_str())])
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("create Docker sandbox failed: {err}"))?;
        let response = docker_api_response("create Docker sandbox", response).await?;
        let container_id = response
            .get("Id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Docker create response did not include a container ID".to_string())?
            .to_string();

        let response = api
            .client
            .post(api.url(format!("/containers/{container_id}/start").as_str()))
            .send()
            .await
            .map_err(|err| format!("start Docker sandbox failed: {err}"))?;
        if let Err(err) = docker_api_empty_response(
            "start Docker sandbox",
            response,
            &[StatusCode::NOT_MODIFIED],
        )
        .await
        {
            let _ = api
                .client
                .delete(api.url(format!("/containers/{container_id}").as_str()))
                .query(&[("force", "true")])
                .send()
                .await;
            return Err(err);
        }

        Ok(SandboxInstance {
            sandbox_id: spec.sandbox_id,
            backend_id: Some(container_id),
            agent_endpoint: self.agent_endpoint(&name, publish_agent).await,
        })
    }

    async fn stop_with_api(&self, sandbox_id: &str) -> Result<(), String> {
        let api = self
            .api
            .as_ref()
            .ok_or_else(|| "Docker API is not configured".to_string())?;
        let name = docker_name(sandbox_id);
        let response = api
            .client
            .post(api.url(format!("/containers/{name}/stop").as_str()))
            .send()
            .await
            .map_err(|err| format!("stop Docker sandbox failed: {err}"))?;
        docker_api_empty_response(
            "stop Docker sandbox",
            response,
            &[StatusCode::NOT_MODIFIED, StatusCode::NOT_FOUND],
        )
        .await
    }

    async fn destroy_with_api(&self, sandbox_id: &str) -> Result<(), String> {
        let api = self
            .api
            .as_ref()
            .ok_or_else(|| "Docker API is not configured".to_string())?;
        let name = docker_name(sandbox_id);
        let response = api
            .client
            .delete(api.url(format!("/containers/{name}").as_str()))
            .query(&[("force", "true")])
            .send()
            .await
            .map_err(|err| format!("remove Docker sandbox failed: {err}"))?;
        docker_api_empty_response("remove Docker sandbox", response, &[StatusCode::NOT_FOUND]).await
    }

    async fn inspect_with_api(&self, sandbox_id: &str) -> Result<Option<SandboxInstance>, String> {
        let name = docker_name(sandbox_id);
        let Some(inspect) = self.inspect_container_with_api(&name).await? else {
            return Ok(None);
        };
        let agent_endpoint = self.agent_endpoint_from_inspect(&name, &inspect);
        Ok(Some(SandboxInstance {
            sandbox_id: sandbox_id.to_string(),
            backend_id: Some(name),
            agent_endpoint,
        }))
    }

    async fn inspect_container_with_api(&self, name: &str) -> Result<Option<Value>, String> {
        let api = self
            .api
            .as_ref()
            .ok_or_else(|| "Docker API is not configured".to_string())?;
        let response = api
            .client
            .get(api.url(format!("/containers/{name}/json").as_str()))
            .send()
            .await
            .map_err(|err| format!("inspect Docker sandbox failed: {err}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        docker_api_response("inspect Docker sandbox", response)
            .await
            .map(Some)
    }

    fn agent_endpoint_from_inspect(&self, name: &str, inspect: &Value) -> Option<String> {
        if self.config.agent_port == 0 {
            return None;
        }
        match self.config.docker_agent_endpoint_mode {
            DockerAgentEndpointMode::Container => {
                Some(format!("http://{}:{}", name, self.config.agent_port))
            }
            DockerAgentEndpointMode::Published if self.config.docker_agent_publish => {
                published_agent_endpoint_from_inspect(
                    inspect,
                    self.config.agent_port,
                    self.config.docker_agent_connect_host.as_str(),
                )
            }
            DockerAgentEndpointMode::Published => None,
        }
    }

    async fn agent_endpoint(&self, name: &str, publish_agent: bool) -> Option<String> {
        if self.config.agent_port == 0 {
            return None;
        }
        match self.config.docker_agent_endpoint_mode {
            DockerAgentEndpointMode::Container => {
                Some(format!("http://{}:{}", name, self.config.agent_port))
            }
            DockerAgentEndpointMode::Published if publish_agent => {
                if self.api.is_some() {
                    let inspect = self.inspect_container_with_api(name).await.ok()??;
                    return published_agent_endpoint_from_inspect(
                        &inspect,
                        self.config.agent_port,
                        self.config.docker_agent_connect_host.as_str(),
                    );
                }
                published_agent_endpoint(
                    "docker",
                    name,
                    self.config.agent_port,
                    self.config.docker_agent_connect_host.as_str(),
                )
                .await
            }
            DockerAgentEndpointMode::Published => None,
        }
    }
}

impl DockerApiClient {
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

fn docker_api_base_url(docker_host: Option<&str>) -> Option<String> {
    let docker_host = docker_host?.trim().trim_end_matches('/');
    if let Some(address) = docker_host.strip_prefix("tcp://") {
        return (!address.is_empty()).then(|| format!("http://{address}"));
    }
    if docker_host.starts_with("http://") || docker_host.starts_with("https://") {
        return Some(docker_host.to_string());
    }
    None
}

async fn docker_api_response(action: &str, response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("{action} returned an unreadable response: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "{action} failed with Docker API status {status}: {}",
            body.trim()
        ));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body)
        .map_err(|err| format!("{action} returned invalid Docker API JSON: {err}"))
}

async fn docker_api_empty_response(
    action: &str,
    response: reqwest::Response,
    allowed_statuses: &[StatusCode],
) -> Result<(), String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("{action} returned an unreadable response: {err}"))?;
    if status.is_success() || allowed_statuses.contains(&status) {
        return Ok(());
    }
    Err(format!(
        "{action} failed with Docker API status {status}: {}",
        body.trim()
    ))
}

fn published_agent_endpoint_from_inspect(
    inspect: &Value,
    agent_port: u16,
    connect_host: &str,
) -> Option<String> {
    let port_pointer = format!("/NetworkSettings/Ports/{agent_port}~1tcp/0/HostPort");
    let host_port = inspect.pointer(port_pointer.as_str())?.as_str()?.trim();
    if host_port.is_empty() {
        return None;
    }
    Some(format!("http://{connect_host}:{host_port}"))
}

fn docker_name(sandbox_id: &str) -> String {
    format!("chatos-sandbox-{sandbox_id}")
}

async fn published_agent_endpoint(
    cli: &str,
    name: &str,
    agent_port: u16,
    connect_host: &str,
) -> Option<String> {
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
    Some(format!("http://{connect_host}:{host_port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tcp_docker_host_for_engine_api() {
        assert_eq!(
            docker_api_base_url(Some("tcp://docker-proxy:2375/")),
            Some("http://docker-proxy:2375".to_string())
        );
        assert_eq!(
            docker_api_base_url(Some("https://docker.example.test/")),
            Some("https://docker.example.test".to_string())
        );
        assert_eq!(
            docker_api_base_url(Some("unix:///var/run/docker.sock")),
            None
        );
        assert_eq!(docker_api_base_url(Some("tcp://")), None);
    }

    #[test]
    fn reads_published_agent_port_from_inspect_response() {
        let inspect = json!({
            "NetworkSettings": {
                "Ports": {
                    "49888/tcp": [{"HostIp": "127.0.0.1", "HostPort": "32768"}]
                }
            }
        });
        assert_eq!(
            published_agent_endpoint_from_inspect(&inspect, 49_888, "docker-host"),
            Some("http://docker-host:32768".to_string())
        );
    }

    #[test]
    fn ignores_missing_published_agent_port() {
        assert_eq!(
            published_agent_endpoint_from_inspect(&json!({}), 49_888, "docker-host"),
            None
        );
    }
}
