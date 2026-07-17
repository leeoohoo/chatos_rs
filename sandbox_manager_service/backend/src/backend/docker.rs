// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::process::Command;

use crate::config::{AppConfig, DockerAgentEndpointMode};

use super::{
    SandboxBackend, SandboxCreateSpec, SandboxEnvironmentCreateSpec, SandboxEnvironmentInstance,
    SandboxEnvironmentServiceInstance, SandboxEnvironmentServiceSpec, SandboxExecResult,
    SandboxInstance,
};

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

    async fn create_environment(
        &self,
        spec: SandboxEnvironmentCreateSpec,
    ) -> Result<SandboxEnvironmentInstance, String> {
        self.create_environment_with_cli(spec).await
    }

    async fn stop_environment(&self, environment_id: &str) -> Result<(), String> {
        for container in environment_container_names(environment_id).await? {
            let output = Command::new("docker")
                .arg("stop")
                .arg(container.as_str())
                .output()
                .await
                .map_err(|err| format!("docker stop environment service failed: {err}"))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.contains("No such container") {
                    return Err(format!("docker stop environment service failed: {stderr}"));
                }
            }
        }
        Ok(())
    }

    async fn start_environment(&self, environment_id: &str) -> Result<(), String> {
        let containers = environment_container_names(environment_id).await?;
        if containers.is_empty() {
            return Err(format!("Docker environment not found: {environment_id}"));
        }
        for container in containers {
            let output = Command::new("docker")
                .arg("start")
                .arg(container.as_str())
                .output()
                .await
                .map_err(|err| format!("docker start environment service failed: {err}"))?;
            if !output.status.success() {
                return Err(format!(
                    "docker start environment service failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
        for dependency in environment_container_names_for_role(environment_id, "dependency").await?
        {
            wait_environment_service_ready(dependency.as_str()).await?;
        }
        Ok(())
    }

    async fn destroy_environment(&self, environment_id: &str) -> Result<(), String> {
        for container in environment_container_names(environment_id).await? {
            let output = Command::new("docker")
                .arg("rm")
                .arg("-f")
                .arg(container.as_str())
                .output()
                .await
                .map_err(|err| format!("docker remove environment service failed: {err}"))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.contains("No such container") {
                    return Err(format!(
                        "docker remove environment service failed: {stderr}"
                    ));
                }
            }
        }
        let network = environment_network_name(environment_id);
        let _ = Command::new("docker")
            .arg("network")
            .arg("rm")
            .arg(network)
            .output()
            .await;
        for volume in environment_volume_names(environment_id).await? {
            let _ = Command::new("docker")
                .arg("volume")
                .arg("rm")
                .arg("-f")
                .arg(volume)
                .output()
                .await;
        }
        for image in environment_image_names(environment_id).await? {
            let _ = Command::new("docker")
                .arg("image")
                .arg("rm")
                .arg("-f")
                .arg(image)
                .output()
                .await;
        }
        let build_root = self
            .config
            .work_root
            .join("environment-builds")
            .join(safe_name(environment_id));
        let _ = std::fs::remove_dir_all(build_root);
        Ok(())
    }

    async fn inspect_environment(
        &self,
        environment_id: &str,
    ) -> Result<Option<SandboxEnvironmentInstance>, String> {
        let containers = environment_container_names(environment_id).await?;
        if containers.is_empty() {
            return Ok(None);
        }
        let mut services = Vec::with_capacity(containers.len());
        for container in containers {
            let Some(service) = self
                .inspect_environment_service(environment_id, container.as_str())
                .await?
            else {
                continue;
            };
            services.push(service);
        }
        Ok(Some(SandboxEnvironmentInstance {
            environment_id: environment_id.to_string(),
            backend_id: Some(environment_network_name(environment_id)),
            services,
        }))
    }

    async fn exec_environment_service(
        &self,
        environment_id: &str,
        service_id: &str,
        command: &[String],
    ) -> Result<SandboxExecResult, String> {
        if command.is_empty() {
            return Err("environment exec command is empty".to_string());
        }
        let name = environment_service_name(environment_id, service_id);
        let output = Command::new("docker")
            .arg("exec")
            .arg(name)
            .args(command)
            .output()
            .await
            .map_err(|err| format!("docker exec environment service failed: {err}"))?;
        Ok(SandboxExecResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

impl DockerSandboxBackend {
    async fn create_environment_with_cli(
        &self,
        spec: SandboxEnvironmentCreateSpec,
    ) -> Result<SandboxEnvironmentInstance, String> {
        let network = environment_network_name(spec.environment_id.as_str());
        let network_output = Command::new("docker")
            .arg("network")
            .arg("create")
            .arg("--label")
            .arg(format!("chatos.environment_id={}", spec.environment_id))
            .arg("--label")
            .arg(format!(
                "com.docker.compose.project={}",
                environment_compose_project_name(spec.environment_id.as_str())
            ))
            .arg("--label")
            .arg("com.docker.compose.network=default")
            .arg(network.as_str())
            .output()
            .await
            .map_err(|err| format!("create Docker environment network failed: {err}"))?;
        if !network_output.status.success() {
            let stderr = String::from_utf8_lossy(&network_output.stderr);
            if !stderr.contains("already exists") {
                return Err(format!(
                    "create Docker environment network failed: {stderr}"
                ));
            }
        }

        let mut ordered_services = spec.services.iter().collect::<Vec<_>>();
        ordered_services.sort_by_key(|service| service.service_role == "application");
        let mut instances = Vec::with_capacity(ordered_services.len());
        for service in ordered_services {
            match self
                .create_environment_service(&spec, service, network.as_str())
                .await
            {
                Ok(instance) => {
                    if service.service_role == "dependency" {
                        let container = environment_service_name(
                            spec.environment_id.as_str(),
                            service.service_id.as_str(),
                        );
                        if let Err(error) = wait_environment_service_ready(container.as_str()).await
                        {
                            let _ = self.destroy_environment(spec.environment_id.as_str()).await;
                            return Err(error);
                        }
                    }
                    instances.push(instance)
                }
                Err(err) => {
                    let _ = self.destroy_environment(spec.environment_id.as_str()).await;
                    return Err(err);
                }
            }
        }
        Ok(SandboxEnvironmentInstance {
            environment_id: spec.environment_id,
            backend_id: Some(network),
            services: instances,
        })
    }

    async fn create_environment_service(
        &self,
        environment: &SandboxEnvironmentCreateSpec,
        service: &SandboxEnvironmentServiceSpec,
        network: &str,
    ) -> Result<SandboxEnvironmentServiceInstance, String> {
        let service_name = environment_service_name(
            environment.environment_id.as_str(),
            service.service_id.as_str(),
        );
        let image = if service.mcp_enabled {
            if let Some(dockerfile) = service.dockerfile.as_deref() {
                self.build_program_managed_application_image(
                    environment.environment_id.as_str(),
                    &service.service_id,
                    environment.run_workspace.as_str(),
                    service.image.as_str(),
                    dockerfile,
                )
                .await?
            } else {
                service.image.clone()
            }
        } else {
            service.image.clone()
        };

        let mut command = Command::new("docker");
        let compose_project = environment_compose_project_name(environment.environment_id.as_str());
        command
            .arg("run")
            .arg("-d")
            .arg("--name")
            .arg(service_name.as_str())
            .arg("--hostname")
            .arg(service.service_id.as_str())
            .arg("--label")
            .arg(format!(
                "chatos.environment_id={}",
                environment.environment_id
            ))
            .arg("--label")
            .arg(format!("chatos.service_id={}", service.service_id))
            .arg("--label")
            .arg(format!("chatos.service_role={}", service.service_role))
            .arg("--label")
            .arg(format!("com.docker.compose.project={compose_project}"))
            .arg("--label")
            .arg(format!("com.docker.compose.service={}", service.service_id))
            .arg("--label")
            .arg("com.docker.compose.container-number=1")
            .arg("--label")
            .arg("com.docker.compose.oneoff=False")
            .arg("--network")
            .arg(network)
            .arg("--network-alias")
            .arg(service.service_id.as_str())
            .arg("--security-opt")
            .arg("no-new-privileges");
        for (name, value) in &service.environment {
            command.arg("-e").arg(format!("{name}={value}"));
        }

        if service.mcp_enabled {
            let cpu = environment.resource_limits.cpu.max(0.1).to_string();
            let memory = format!("{}m", environment.resource_limits.memory_mb.max(128));
            let pids = environment
                .resource_limits
                .max_processes
                .max(16)
                .to_string();
            command
                .arg("--cpus")
                .arg(cpu)
                .arg("--memory")
                .arg(memory)
                .arg("--pids-limit")
                .arg(pids)
                .arg("--workdir")
                .arg("/workspace")
                .arg("--read-only")
                .arg("--cap-drop")
                .arg("ALL")
                .arg("--user")
                .arg("1000:1000")
                .arg("--tmpfs")
                .arg("/tmp:rw,nosuid,nodev,size=256m,mode=1777")
                .arg("--tmpfs")
                .arg("/home/sandbox:rw,nosuid,nodev,size=256m,uid=1000,gid=1000,mode=0700")
                .arg("-v")
                .arg(format!("{}:/workspace:rw", environment.run_workspace))
                .arg("-e")
                .arg(format!(
                    "CHATOS_SANDBOX_ID={}:{}",
                    environment.environment_id, service.service_id
                ))
                .arg("-e")
                .arg(format!(
                    "CHATOS_SANDBOX_MCP_TOKEN={}",
                    environment.agent_token
                ))
                .arg("-e")
                .arg("CHATOS_AGENT_HOST=0.0.0.0")
                .arg("-e")
                .arg(format!("CHATOS_AGENT_PORT={}", self.config.agent_port))
                .arg("-e")
                .arg("CHATOS_WORKSPACE=/workspace")
                .arg("-p")
                .arg(format!(
                    "{}::{}",
                    self.config.docker_agent_bind_host, self.config.agent_port
                ));
        } else if let Some((volume_name, mount_path)) =
            dependency_volume(&environment.environment_id, &service.service_id)
        {
            ensure_environment_volume(environment.environment_id.as_str(), volume_name.as_str())
                .await?;
            command.arg("-v").arg(format!("{volume_name}:{mount_path}"));
        }
        append_dependency_healthcheck(&mut command, service);
        command.arg(image.as_str());
        append_dependency_command(&mut command, service);
        let output = command
            .output()
            .await
            .map_err(|err| format!("start Docker environment service failed: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "start Docker environment service {} failed: {}",
                service.service_id,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let backend_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let agent_endpoint = if service.mcp_enabled {
            published_agent_endpoint(
                "docker",
                service_name.as_str(),
                self.config.agent_port,
                self.config.docker_agent_connect_host.as_str(),
            )
            .await
        } else {
            None
        };
        Ok(SandboxEnvironmentServiceInstance {
            service_id: service.service_id.clone(),
            backend_id: Some(backend_id),
            status: "running".to_string(),
            agent_endpoint,
            image_ref: image,
        })
    }

    async fn build_program_managed_application_image(
        &self,
        environment_id: &str,
        service_id: &str,
        run_workspace: &str,
        agent_image: &str,
        dockerfile: &str,
    ) -> Result<String, String> {
        let build_root = self
            .config
            .work_root
            .join("environment-builds")
            .join(safe_name(environment_id))
            .join(safe_name(service_id));
        std::fs::create_dir_all(build_root.as_path())
            .map_err(|err| format!("create environment build directory failed: {err}"))?;
        let application_dockerfile = build_root.join("Dockerfile.application");
        std::fs::write(application_dockerfile.as_path(), dockerfile)
            .map_err(|err| format!("write application Dockerfile failed: {err}"))?;
        let application_image = format!(
            "chatos-environment-{}-{}-application:latest",
            safe_name(environment_id),
            safe_name(service_id)
        );
        run_docker_build(
            application_dockerfile.as_path(),
            Path::new(run_workspace),
            application_image.as_str(),
        )
        .await?;
        let original_command = inspect_image_command(application_image.as_str()).await?;
        let launcher_path = build_root.join("chatos-runtime-launcher");
        std::fs::write(launcher_path.as_path(), program_managed_launcher_script())
            .map_err(|err| format!("write program-managed runtime launcher failed: {err}"))?;
        let wrapper_dockerfile = build_root.join("Dockerfile");
        let command_json = serde_json::to_string(&original_command)
            .map_err(|err| format!("encode application command failed: {err}"))?;
        let wrapper = format!(
            "FROM {agent_image} AS chatos_agent\nFROM {application_image}\nCOPY --from=chatos_agent /usr/local/bin/chatos-sandbox-mcp-server /usr/local/bin/chatos-sandbox-mcp-server\nCOPY chatos-runtime-launcher /usr/local/bin/chatos-runtime-launcher\nRUN chmod 0555 /usr/local/bin/chatos-sandbox-mcp-server /usr/local/bin/chatos-runtime-launcher\nENV CHATOS_AGENT_HOST=0.0.0.0 CHATOS_AGENT_PORT={} CHATOS_WORKSPACE=/workspace\nENTRYPOINT [\"/usr/local/bin/chatos-runtime-launcher\"]\nCMD {command_json}\n",
            self.config.agent_port
        );
        std::fs::write(wrapper_dockerfile.as_path(), wrapper)
            .map_err(|err| format!("write program-managed wrapper Dockerfile failed: {err}"))?;
        let final_image = format!(
            "chatos-environment-{}-{}:latest",
            safe_name(environment_id),
            safe_name(service_id)
        );
        run_docker_build(
            wrapper_dockerfile.as_path(),
            build_root.as_path(),
            final_image.as_str(),
        )
        .await?;
        Ok(final_image)
    }

    async fn inspect_environment_service(
        &self,
        environment_id: &str,
        container: &str,
    ) -> Result<Option<SandboxEnvironmentServiceInstance>, String> {
        let output = Command::new("docker")
            .arg("inspect")
            .arg(container)
            .output()
            .await
            .map_err(|err| format!("inspect Docker environment service failed: {err}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let values = serde_json::from_slice::<Value>(&output.stdout)
            .map_err(|err| format!("parse Docker environment inspect failed: {err}"))?;
        let inspect = values
            .as_array()
            .and_then(|values| values.first())
            .ok_or_else(|| "Docker environment inspect returned no record".to_string())?;
        let service_id = inspect
            .pointer("/Config/Labels/chatos.service_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Docker environment service label is missing".to_string())?;
        let mcp_enabled = inspect
            .pointer("/Config/Labels/chatos.service_role")
            .and_then(Value::as_str)
            .is_some_and(|role| role == "application");
        let running = inspect
            .pointer("/State/Running")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let status = if running {
            inspect
                .pointer("/State/Health/Status")
                .and_then(Value::as_str)
                .unwrap_or("running")
        } else {
            inspect
                .pointer("/State/Status")
                .and_then(Value::as_str)
                .unwrap_or("stopped")
        };
        let image_ref = inspect
            .pointer("/Config/Image")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let agent_endpoint = if mcp_enabled && running {
            published_agent_endpoint(
                "docker",
                container,
                self.config.agent_port,
                self.config.docker_agent_connect_host.as_str(),
            )
            .await
        } else {
            None
        };
        let _ = environment_id;
        Ok(Some(SandboxEnvironmentServiceInstance {
            service_id: service_id.to_string(),
            backend_id: inspect
                .get("Id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            status: status.to_string(),
            agent_endpoint,
            image_ref,
        }))
    }

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

fn safe_name(value: &str) -> String {
    let mut value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while value.contains("--") {
        value = value.replace("--", "-");
    }
    value.trim_matches('-').to_string()
}

fn environment_network_name(environment_id: &str) -> String {
    format!(
        "{}_default",
        environment_compose_project_name(environment_id)
    )
}

fn environment_compose_project_name(environment_id: &str) -> String {
    format!("chatos-{}", safe_name(environment_id))
}

fn environment_service_name(environment_id: &str, service_id: &str) -> String {
    format!(
        "{}-{}-1",
        environment_compose_project_name(environment_id),
        safe_name(service_id)
    )
}

async fn environment_container_names(environment_id: &str) -> Result<Vec<String>, String> {
    let output = Command::new("docker")
        .arg("ps")
        .arg("-a")
        .arg("--filter")
        .arg(format!("label=chatos.environment_id={environment_id}"))
        .arg("--format")
        .arg("{{.Names}}")
        .output()
        .await
        .map_err(|err| format!("list Docker environment services failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "list Docker environment services failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

async fn environment_container_names_for_role(
    environment_id: &str,
    service_role: &str,
) -> Result<Vec<String>, String> {
    let output = Command::new("docker")
        .arg("ps")
        .arg("-a")
        .arg("--filter")
        .arg(format!("label=chatos.environment_id={environment_id}"))
        .arg("--filter")
        .arg(format!("label=chatos.service_role={service_role}"))
        .arg("--format")
        .arg("{{.Names}}")
        .output()
        .await
        .map_err(|err| format!("list Docker environment services by role failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "list Docker environment services by role failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output_lines(&output.stdout))
}

async fn ensure_environment_volume(environment_id: &str, volume_name: &str) -> Result<(), String> {
    let output = Command::new("docker")
        .arg("volume")
        .arg("create")
        .arg("--label")
        .arg(format!("chatos.environment_id={environment_id}"))
        .arg("--label")
        .arg(format!(
            "com.docker.compose.project={}",
            environment_compose_project_name(environment_id)
        ))
        .arg(volume_name)
        .output()
        .await
        .map_err(|err| format!("create Docker environment volume failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "create Docker environment volume failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

async fn environment_volume_names(environment_id: &str) -> Result<Vec<String>, String> {
    docker_resource_names(
        &["volume", "ls"],
        environment_id,
        "list Docker environment volumes",
    )
    .await
}

async fn environment_image_names(environment_id: &str) -> Result<Vec<String>, String> {
    let output = Command::new("docker")
        .arg("image")
        .arg("ls")
        .arg("--filter")
        .arg(format!(
            "reference=chatos-environment-{}-*",
            safe_name(environment_id)
        ))
        .arg("--format")
        .arg("{{.Repository}}:{{.Tag}}")
        .output()
        .await
        .map_err(|err| format!("list Docker environment images failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "list Docker environment images failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output_lines(&output.stdout))
}

async fn docker_resource_names(
    resource_args: &[&str],
    environment_id: &str,
    action: &str,
) -> Result<Vec<String>, String> {
    let output = Command::new("docker")
        .args(resource_args)
        .arg("--filter")
        .arg(format!("label=chatos.environment_id={environment_id}"))
        .arg("--format")
        .arg("{{.Name}}")
        .output()
        .await
        .map_err(|err| format!("{action} failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{action} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output_lines(&output.stdout))
}

fn output_lines(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn program_managed_launcher_script() -> &'static str {
    "#!/bin/sh\nset -u\nif [ \"$#\" -eq 0 ]; then\n  exec /usr/local/bin/chatos-sandbox-mcp-server\nfi\n\"$@\" &\napp_pid=$!\n/usr/local/bin/chatos-sandbox-mcp-server &\nagent_pid=$!\nterminate() {\n  kill \"$app_pid\" \"$agent_pid\" 2>/dev/null || true\n}\ntrap terminate INT TERM HUP\nwhile kill -0 \"$app_pid\" 2>/dev/null && kill -0 \"$agent_pid\" 2>/dev/null; do\n  sleep 1\ndone\nif kill -0 \"$app_pid\" 2>/dev/null; then\n  wait \"$agent_pid\"\n  status=$?\n  kill \"$app_pid\" 2>/dev/null || true\n  wait \"$app_pid\" 2>/dev/null || true\nelse\n  wait \"$app_pid\"\n  status=$?\n  kill \"$agent_pid\" 2>/dev/null || true\n  wait \"$agent_pid\" 2>/dev/null || true\nfi\nexit \"$status\"\n"
}

fn dependency_volume(environment_id: &str, service_id: &str) -> Option<(String, &'static str)> {
    let mount_path = match service_id {
        "mysql" => "/var/lib/mysql",
        "mongodb" | "mongo" => "/data/db",
        "postgres" | "postgresql" => "/var/lib/postgresql/data",
        "redis" => "/data",
        "nacos" => "/home/nacos/data",
        "rabbitmq" => "/var/lib/rabbitmq",
        "kafka" => "/bitnami/kafka",
        "elasticsearch" | "opensearch" => "/usr/share/elasticsearch/data",
        "minio" => "/data",
        _ => return None,
    };
    Some((
        format!(
            "chatos-env-{}-{}-data",
            safe_name(environment_id),
            safe_name(service_id)
        ),
        mount_path,
    ))
}

fn append_dependency_command(command: &mut Command, service: &SandboxEnvironmentServiceSpec) {
    match service.service_id.as_str() {
        "redis" => {
            command.arg("redis-server").arg("--appendonly").arg("yes");
            if let Some(password) = service
                .environment
                .get("REDIS_PASSWORD")
                .filter(|value| !value.is_empty())
            {
                command.arg("--requirepass").arg(password);
            }
        }
        "minio" => {
            command
                .arg("server")
                .arg("/data")
                .arg("--console-address")
                .arg(":9001");
        }
        _ => {}
    }
}

fn append_dependency_healthcheck(command: &mut Command, service: &SandboxEnvironmentServiceSpec) {
    let health_command = match service.service_id.as_str() {
        "mysql" => Some("mysqladmin ping -h 127.0.0.1 -p\"$MYSQL_ROOT_PASSWORD\" --silent"),
        "mongodb" | "mongo" => {
            Some("mongosh --quiet --eval 'quit(db.runCommand({ ping: 1 }).ok ? 0 : 1)'")
        }
        "postgres" | "postgresql" => Some("pg_isready -U \"$POSTGRES_USER\" -d \"$POSTGRES_DB\""),
        "redis" => Some("if [ -n \"${REDIS_PASSWORD:-}\" ]; then redis-cli -a \"$REDIS_PASSWORD\" ping; else redis-cli ping; fi | grep PONG"),
        "nacos" => Some("curl -fsS http://127.0.0.1:8848/nacos/ >/dev/null"),
        "rabbitmq" => Some("rabbitmq-diagnostics -q ping"),
        "kafka" => Some("kafka-topics.sh --bootstrap-server 127.0.0.1:9092 --list >/dev/null 2>&1"),
        "elasticsearch" | "opensearch" => {
            Some("curl -fsS http://127.0.0.1:9200/_cluster/health >/dev/null")
        }
        "minio" => Some("curl -fsS http://127.0.0.1:9000/minio/health/live >/dev/null"),
        _ => None,
    };
    if let Some(health_command) = health_command {
        command
            .arg("--health-cmd")
            .arg(health_command)
            .arg("--health-interval")
            .arg("5s")
            .arg("--health-timeout")
            .arg("5s")
            .arg("--health-retries")
            .arg("60")
            .arg("--health-start-period")
            .arg("10s");
    }
}

async fn wait_environment_service_ready(container: &str) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        let output = Command::new("docker")
            .arg("inspect")
            .arg(container)
            .output()
            .await
            .map_err(|err| format!("inspect Docker dependency health failed: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "inspect Docker dependency health failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let values = serde_json::from_slice::<Value>(&output.stdout)
            .map_err(|err| format!("parse Docker dependency health failed: {err}"))?;
        let inspect = values
            .as_array()
            .and_then(|values| values.first())
            .ok_or_else(|| "Docker dependency inspect returned no record".to_string())?;
        let running = inspect
            .pointer("/State/Running")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let health = inspect
            .pointer("/State/Health/Status")
            .and_then(Value::as_str);
        if running && matches!(health, None | Some("healthy")) {
            return Ok(());
        }
        if !running || health == Some("unhealthy") {
            return Err(format!(
                "Docker dependency {container} failed health check: running={running}, health={}",
                health.unwrap_or("none")
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "Docker dependency {container} did not become healthy before timeout"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_docker_build(dockerfile: &Path, context: &Path, image: &str) -> Result<(), String> {
    let output = Command::new("docker")
        .arg("build")
        .arg("--file")
        .arg(dockerfile)
        .arg("--tag")
        .arg(image)
        .arg(context)
        .output()
        .await
        .map_err(|err| format!("Docker environment image build failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "Docker environment image build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

async fn inspect_image_command(image: &str) -> Result<Vec<String>, String> {
    let output = Command::new("docker")
        .arg("image")
        .arg("inspect")
        .arg(image)
        .output()
        .await
        .map_err(|err| format!("inspect application image failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "inspect application image failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let values = serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|err| format!("parse application image inspect failed: {err}"))?;
    let config = values
        .as_array()
        .and_then(|values| values.first())
        .and_then(|value| value.get("Config"))
        .ok_or_else(|| "application image inspect has no Config".to_string())?;
    let mut command = json_string_array(config.get("Entrypoint"));
    command.extend(json_string_array(config.get("Cmd")));
    Ok(command)
}

fn json_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
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
    use crate::models::{NetworkPolicy, ResourceLimits};
    use std::collections::BTreeMap;

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

    #[test]
    fn environment_names_and_labels_follow_compose_project_shape() {
        assert_eq!(
            environment_compose_project_name("environment_ABC"),
            "chatos-environment_abc"
        );
        assert_eq!(
            environment_network_name("environment_ABC"),
            "chatos-environment_abc_default"
        );
        assert_eq!(
            environment_service_name("environment_ABC", "services-api"),
            "chatos-environment_abc-services-api-1"
        );
    }

    #[test]
    fn launcher_stops_agent_when_application_exits_and_vice_versa() {
        let script = program_managed_launcher_script();
        assert!(script.contains("app_pid=$!"));
        assert!(script.contains("agent_pid=$!"));
        assert!(script.contains("kill \"$app_pid\""));
        assert!(script.contains("kill \"$agent_pid\""));
        assert!(script.contains("trap terminate INT TERM HUP"));
    }

    #[tokio::test]
    #[ignore = "requires Docker and a locally built chatos-sandbox-agent:latest image"]
    async fn docker_environment_smoke_groups_services_and_keeps_dependency_agent_free() {
        let mut config = AppConfig::from_env().expect("config");
        config.docker_image = "chatos-sandbox-agent:latest".to_string();
        let environment_id = format!("smoke-{}", uuid::Uuid::new_v4().simple());
        let work_root = std::env::temp_dir().join(format!("chatos-{environment_id}"));
        let workspace = work_root.join("workspace");
        std::fs::create_dir_all(workspace.as_path()).expect("workspace");
        std::fs::write(workspace.join("README.md"), "smoke\n").expect("source");
        config.work_root = work_root.clone();
        let backend = DockerSandboxBackend::new(config.clone());
        let mut dependency_environment = BTreeMap::new();
        dependency_environment.insert(
            "MONGO_INITDB_ROOT_USERNAME".to_string(),
            "smoke-user".to_string(),
        );
        dependency_environment.insert(
            "MONGO_INITDB_ROOT_PASSWORD".to_string(),
            "smoke-secret".to_string(),
        );
        dependency_environment.insert("MONGO_INITDB_DATABASE".to_string(), "smoke".to_string());
        let result: Result<(), String> = async {
            let instance = backend
                .create_environment(SandboxEnvironmentCreateSpec {
                    environment_id: environment_id.clone(),
                    run_workspace: workspace.to_string_lossy().to_string(),
                    services: vec![
                        SandboxEnvironmentServiceSpec {
                            service_id: "mongodb".to_string(),
                            service_role: "dependency".to_string(),
                            image: "mongo:7.0".to_string(),
                            dockerfile: None,
                            environment: dependency_environment,
                            mcp_enabled: false,
                        },
                        SandboxEnvironmentServiceSpec {
                            service_id: "api".to_string(),
                            service_role: "application".to_string(),
                            image: config.docker_image.clone(),
                            dockerfile: Some(
                                "FROM mongo:7.0\nENTRYPOINT []\nCMD [\"sh\", \"-c\", \"while true; do sleep 3600; done\"]\n"
                                    .to_string(),
                            ),
                            environment: BTreeMap::new(),
                            mcp_enabled: true,
                        },
                    ],
                    agent_token: "smoke-program-token".to_string(),
                    resource_limits: ResourceLimits {
                        cpu: 1.0,
                        memory_mb: 512,
                        disk_mb: 1024,
                        max_processes: 64,
                    },
                    network: NetworkPolicy::default(),
                })
                .await?;
            let dependency = instance
                .services
                .iter()
                .find(|service| service.service_id == "mongodb")
                .ok_or_else(|| "mongodb service missing".to_string())?;
            if dependency.agent_endpoint.is_some() {
                return Err("dependency unexpectedly received an Agent endpoint".to_string());
            }
            let application = instance
                .services
                .iter()
                .find(|service| service.service_id == "api")
                .ok_or_else(|| "api service missing".to_string())?;
            if application.agent_endpoint.is_none() {
                return Err("application Agent endpoint is missing".to_string());
            }
            let dependency_inspect = docker_inspect_value(
                environment_service_name(environment_id.as_str(), "mongodb").as_str(),
            )
            .await?;
            let dependency_env = json_string_array(dependency_inspect.pointer("/Config/Env"));
            if dependency_env
                .iter()
                .any(|value| value.starts_with("CHATOS_SANDBOX_MCP_"))
            {
                return Err("dependency received program-managed MCP environment".to_string());
            }
            let compose_project = dependency_inspect
                .pointer("/Config/Labels/com.docker.compose.project")
                .and_then(Value::as_str);
            if compose_project != Some(environment_compose_project_name(&environment_id).as_str())
            {
                return Err("Compose parent label is missing".to_string());
            }
            let exec = backend
                .exec_environment_service(
                    environment_id.as_str(),
                    "api",
                    &[
                        "sh".to_string(),
                        "-c".to_string(),
                        "test -x /usr/local/bin/chatos-sandbox-mcp-server".to_string(),
                    ],
                )
                .await?;
            if exec.exit_code != 0 {
                return Err(format!("application Agent binary check failed: {}", exec.stderr));
            }
            backend.stop_environment(environment_id.as_str()).await?;
            backend.start_environment(environment_id.as_str()).await?;
            Ok(())
        }
        .await;
        let cleanup = backend.destroy_environment(environment_id.as_str()).await;
        let _ = std::fs::remove_dir_all(work_root);
        result.expect("Docker environment smoke result");
        cleanup.expect("Docker environment smoke cleanup");
    }
}

#[cfg(test)]
async fn docker_inspect_value(name: &str) -> Result<Value, String> {
    let output = Command::new("docker")
        .arg("inspect")
        .arg(name)
        .output()
        .await
        .map_err(|err| format!("docker inspect failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "docker inspect failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|err| format!("parse docker inspect failed: {err}"))?
        .as_array()
        .and_then(|values| values.first())
        .cloned()
        .ok_or_else(|| "docker inspect returned no record".to_string())
}
