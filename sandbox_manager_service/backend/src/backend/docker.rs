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
    append_sandbox_create_runtime_args, SandboxBackend, SandboxCreateSpec,
    SandboxEnvironmentCreateSpec, SandboxEnvironmentInstance, SandboxEnvironmentServiceInstance,
    SandboxEnvironmentServiceSpec, SandboxExecResult, SandboxInstance,
};

mod runtime;

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

fn enable_docker_init(command: &mut Command) {
    command.arg("--init");
}

fn enable_docker_api_init(payload: &mut Value) {
    payload["HostConfig"]["Init"] = json!(true);
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

async fn run_docker_build(
    dockerfile: &Path,
    context: &Path,
    image: &str,
    docker_config: Option<&Path>,
    docker_host: Option<&str>,
) -> Result<(), String> {
    const BUILD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);
    let mut command = Command::new("docker");
    command
        .arg("build")
        .arg("--file")
        .arg(dockerfile)
        .arg("--tag")
        .arg(image)
        .arg(context)
        .kill_on_drop(true);
    if let Some(docker_config) = docker_config {
        std::fs::create_dir_all(docker_config)
            .map_err(|err| format!("create Docker config directory failed: {err}"))?;
        command.env("DOCKER_CONFIG", docker_config);
    }
    if let Some(docker_host) = docker_host {
        command.env("DOCKER_HOST", docker_host);
    }
    let output = tokio::time::timeout(BUILD_TIMEOUT, command.output())
        .await
        .map_err(|_| {
            format!(
                "Docker environment image build timed out after {} seconds",
                BUILD_TIMEOUT.as_secs()
            )
        })?
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
include!("docker.test.rs");
