// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl DockerSandboxBackend {
    pub(super) async fn create_environment_with_cli(
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

    pub(super) async fn create_environment_service(
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
        command.arg("run").arg("-d");
        enable_docker_init(&mut command);
        command
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

    pub(super) async fn build_program_managed_application_image(
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
            self.config.docker_config.as_deref(),
            self.config.docker_host.as_deref(),
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
            self.config.docker_config.as_deref(),
            self.config.docker_host.as_deref(),
        )
        .await?;
        Ok(final_image)
    }

    pub(super) async fn inspect_environment_service(
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

    pub(super) async fn create_with_cli(
        &self,
        spec: SandboxCreateSpec,
    ) -> Result<SandboxInstance, String> {
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
        command.arg("run").arg("-d");
        enable_docker_init(&mut command);
        command
            .arg("--name")
            .arg(&name)
            .arg("--hostname")
            .arg(&name)
            .arg("--label")
            .arg(format!("chatos.sandbox_id={}", spec.sandbox_id))
            .arg("--label")
            .arg("chatos.backend=docker");
        append_sandbox_create_runtime_args(
            &mut command,
            &spec,
            network,
            cpu.as_str(),
            memory.as_str(),
            pids.as_str(),
            disk_limit_bytes,
        );
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

    pub(super) async fn create_with_api(
        &self,
        spec: SandboxCreateSpec,
    ) -> Result<SandboxInstance, String> {
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
        enable_docker_api_init(&mut payload);
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

    pub(super) async fn stop_with_api(&self, sandbox_id: &str) -> Result<(), String> {
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

    pub(super) async fn destroy_with_api(&self, sandbox_id: &str) -> Result<(), String> {
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

    pub(super) async fn inspect_with_api(
        &self,
        sandbox_id: &str,
    ) -> Result<Option<SandboxInstance>, String> {
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

    pub(super) async fn inspect_container_with_api(
        &self,
        name: &str,
    ) -> Result<Option<Value>, String> {
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

    pub(super) fn agent_endpoint_from_inspect(
        &self,
        name: &str,
        inspect: &Value,
    ) -> Option<String> {
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

    pub(super) async fn agent_endpoint(&self, name: &str, publish_agent: bool) -> Option<String> {
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
