// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Component, Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::sandbox::docker::{docker_command, ensure_docker_running};
use crate::LocalState;

const RUNTIME_DIRECTORY: &str = ".chatos/runtime-environment";
const COMPOSE_FILE_NAME: &str = "docker-compose.chatos.yml";
const APPLICATION_DOCKERFILE_NAME: &str = "Dockerfile.application";
const APPLICATION_DOCKERIGNORE_NAME: &str = "Dockerfile.application.dockerignore";
const ENV_FILE_NAME: &str = ".env.chatos";
const MAX_COMPOSE_BYTES: usize = 1024 * 1024;
const MAX_DOCKERFILE_BYTES: usize = 512 * 1024;
const MAX_ENV_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct ComposeUpRequest {
    pub(crate) project_name: String,
    #[serde(default)]
    pub(crate) project_relative_path: Option<String>,
    pub(crate) compose_yaml: String,
    pub(crate) application_dockerfile: String,
    pub(crate) env_file: String,
}

pub(crate) async fn start_project_compose_environment(
    state: &LocalState,
    workspace_id: &str,
    body: Value,
) -> Result<Value> {
    let input: ComposeUpRequest =
        serde_json::from_value(body).context("parse Docker Compose request")?;
    let project_name = validate_project_name(input.project_name.as_str())?;
    validate_generated_content(&input)?;
    ensure_docker_running().await?;

    let project_root =
        resolve_project_root(state, workspace_id, input.project_relative_path.as_deref()).await?;
    let runtime_directory = project_root.join(RUNTIME_DIRECTORY);
    tokio::fs::create_dir_all(runtime_directory.as_path())
        .await
        .with_context(|| format!("create runtime directory {}", runtime_directory.display()))?;
    let compose_path = runtime_directory.join(COMPOSE_FILE_NAME);
    let dockerfile_path = runtime_directory.join(APPLICATION_DOCKERFILE_NAME);
    let dockerignore_path = runtime_directory.join(APPLICATION_DOCKERIGNORE_NAME);
    let env_path = runtime_directory.join(ENV_FILE_NAME);
    tokio::fs::write(compose_path.as_path(), input.compose_yaml.as_bytes())
        .await
        .with_context(|| format!("write {}", compose_path.display()))?;
    tokio::fs::write(
        dockerfile_path.as_path(),
        input.application_dockerfile.as_bytes(),
    )
    .await
    .with_context(|| format!("write {}", dockerfile_path.display()))?;
    tokio::fs::write(
        dockerignore_path.as_path(),
        b".chatos/runtime-environment/.env.chatos\n",
    )
    .await
    .with_context(|| format!("write {}", dockerignore_path.display()))?;
    tokio::fs::write(env_path.as_path(), input.env_file.as_bytes())
        .await
        .with_context(|| format!("write {}", env_path.display()))?;
    restrict_generated_file(env_path.as_path()).await?;

    let normalized_compose = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name,
        &["config", "--format", "json"],
    )
    .await
    .context("validate generated Docker Compose file")?;
    let normalized_compose = serde_json::from_str::<Value>(normalized_compose.as_str())
        .context("parse normalized Docker Compose configuration")?;
    validate_normalized_compose(project_root.as_path(), &normalized_compose)?;
    let up_output = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name,
        &["up", "-d", "--build", "--remove-orphans"],
    )
    .await
    .context("start project Docker Compose environment")?;
    let ps_output = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name,
        &["ps", "--format", "json"],
    )
    .await
    .unwrap_or_default();
    Ok(json!({
        "project_name": project_name,
        "status": "running",
        "runtime_directory": runtime_directory.to_string_lossy(),
        "compose_file": compose_path.to_string_lossy(),
        "output": up_output,
        "services": parse_compose_ps(ps_output.as_str()),
    }))
}

async fn resolve_project_root(
    state: &LocalState,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> Result<PathBuf> {
    let workspace = state
        .workspace_by_id(workspace_id)
        .ok_or_else(|| anyhow!("workspace not found: {workspace_id}"))?;
    let workspace_root = tokio::fs::canonicalize(workspace.absolute_root.as_path())
        .await
        .with_context(|| {
            format!(
                "resolve workspace root {}",
                workspace.absolute_root.display()
            )
        })?;
    let relative_path = normalize_relative_path(relative_path.unwrap_or(""))?;
    let candidate = workspace_root.join(relative_path);
    let project_root = tokio::fs::canonicalize(candidate.as_path())
        .await
        .with_context(|| format!("resolve project root {}", candidate.display()))?;
    if !project_root.starts_with(workspace_root.as_path()) || !project_root.is_dir() {
        return Err(anyhow!("project root escapes the authorized workspace"));
    }
    Ok(project_root)
}

fn normalize_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value.trim());
    if path.is_absolute() {
        return Err(anyhow!("project_relative_path must be workspace-relative"));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            _ => return Err(anyhow!("project_relative_path contains an unsafe segment")),
        }
    }
    Ok(normalized)
}

fn validate_project_name(value: &str) -> Result<&str> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 63
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        || !value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        return Err(anyhow!("invalid Docker Compose project name"));
    }
    Ok(value)
}

fn validate_generated_content(input: &ComposeUpRequest) -> Result<()> {
    if input.compose_yaml.is_empty() || input.compose_yaml.len() > MAX_COMPOSE_BYTES {
        return Err(anyhow!(
            "generated Docker Compose file is empty or too large"
        ));
    }
    if input.application_dockerfile.is_empty()
        || input.application_dockerfile.len() > MAX_DOCKERFILE_BYTES
    {
        return Err(anyhow!(
            "generated application Dockerfile is empty or too large"
        ));
    }
    if input.env_file.len() > MAX_ENV_BYTES {
        return Err(anyhow!("generated environment file is too large"));
    }
    validate_compose_source_before_resolution(input.compose_yaml.as_str())?;
    let compose_lower = input.compose_yaml.to_ascii_lowercase();
    for forbidden in [
        "privileged: true",
        "network_mode: host",
        "pid: host",
        "ipc: host",
        "/var/run/docker.sock",
        "cap_add:",
        "devices:",
    ] {
        if compose_lower.contains(forbidden) {
            return Err(anyhow!(
                "generated Docker Compose file contains forbidden setting: {forbidden}"
            ));
        }
    }
    if !compose_lower.contains("services:") || !compose_lower.contains("application:") {
        return Err(anyhow!(
            "generated Docker Compose file must contain the application service"
        ));
    }
    Ok(())
}

fn validate_compose_source_before_resolution(source: &str) -> Result<()> {
    if source.contains('\0') {
        return Err(anyhow!("generated Docker Compose file contains a NUL byte"));
    }
    let mut env_file_block_indent = None;
    for raw_line in source.lines() {
        if raw_line
            .chars()
            .take_while(|character| character.is_whitespace())
            .any(|character| character == '\t')
        {
            return Err(anyhow!(
                "generated Docker Compose file must use spaces for indentation"
            ));
        }
        let indent = raw_line
            .chars()
            .take_while(|character| *character == ' ')
            .count();
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let unquoted = yaml_line_without_quoted_content(trimmed);
        if unquoted.contains('{')
            || unquoted.contains('}')
            || unquoted.contains('!')
            || unquoted.trim_start().starts_with('?')
            || unquoted.split_whitespace().any(|token| {
                token.starts_with('&') || token.starts_with('*') || token.starts_with("<<:")
            })
        {
            return Err(anyhow!(
                "generated Docker Compose file cannot use YAML flow mappings, anchors, aliases, or merge keys"
            ));
        }

        if let Some(block_indent) = env_file_block_indent {
            if indent > block_indent {
                let value = trimmed
                    .strip_prefix('-')
                    .map(str::trim)
                    .ok_or_else(|| anyhow!("env_file entries must be a simple YAML list"))?;
                validate_env_file_value(value)?;
                continue;
            }
            env_file_block_indent = None;
        }

        let Some((raw_key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        if raw_key.contains('\'') || raw_key.contains('"') {
            return Err(anyhow!(
                "generated Docker Compose file cannot use quoted or escaped mapping keys"
            ));
        }
        let key = raw_key
            .trim()
            .trim_matches(['\'', '"'])
            .to_ascii_lowercase();
        if matches!(
            key.as_str(),
            "include" | "extends" | "configs" | "secrets" | "<<"
        ) {
            return Err(anyhow!(
                "generated Docker Compose file contains forbidden directive: {key}"
            ));
        }
        if key == "env_file" {
            let value = raw_value.trim();
            if value.is_empty() {
                env_file_block_indent = Some(indent);
            } else {
                let value = value
                    .strip_prefix('[')
                    .and_then(|value| value.strip_suffix(']'))
                    .unwrap_or(value);
                for entry in value.split(',') {
                    validate_env_file_value(entry)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_env_file_value(value: &str) -> Result<()> {
    let value = value.trim().trim_matches(['\'', '"']).trim();
    if value == ".env.chatos" {
        Ok(())
    } else {
        Err(anyhow!(
            "generated Docker Compose env_file must be exactly .env.chatos"
        ))
    }
}

fn yaml_line_without_quoted_content(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut quote = None;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
            result.push(' ');
            continue;
        }
        if quote == Some('"') && character == '\\' {
            escaped = true;
            result.push(' ');
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            result.push(' ');
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
            result.push(' ');
        } else if character == '#' {
            break;
        } else {
            result.push(character);
        }
    }
    result
}

fn validate_normalized_compose(project_root: &Path, compose: &Value) -> Result<()> {
    let root = compose
        .as_object()
        .ok_or_else(|| anyhow!("normalized Docker Compose configuration must be an object"))?;
    let services = root
        .get("services")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("normalized Docker Compose configuration has no services"))?;
    for (service_name, service) in services {
        let service = service
            .as_object()
            .ok_or_else(|| anyhow!("Docker Compose service {service_name} must be an object"))?;
        for forbidden in [
            "cap_add",
            "cgroup",
            "cgroup_parent",
            "credential_spec",
            "device_cgroup_rules",
            "devices",
            "gpus",
            "ipc",
            "isolation",
            "network_mode",
            "pid",
            "privileged",
            "runtime",
            "security_opt",
            "sysctls",
            "use_api_socket",
            "userns_mode",
            "uts",
            "volumes_from",
        ] {
            if service.get(forbidden).is_some_and(compose_setting_enabled) {
                return Err(anyhow!(
                    "Docker Compose service {service_name} contains forbidden setting: {forbidden}"
                ));
            }
        }
        if let Some(build) = service.get("build") {
            validate_compose_build(project_root, service_name, build)?;
        }
        if let Some(volumes) = service.get("volumes") {
            validate_compose_volumes(service_name, volumes)?;
        }
        if let Some(ports) = service.get("ports") {
            validate_compose_ports(service_name, ports)?;
        }
    }
    for section in ["networks", "volumes"] {
        for (name, resource) in root
            .get(section)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if resource
                .get("external")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(anyhow!(
                    "Docker Compose {section} resource {name} cannot be external"
                ));
            }
        }
    }
    Ok(())
}

fn compose_setting_enabled(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(_) => true,
    }
}

fn validate_compose_build(project_root: &Path, service_name: &str, build: &Value) -> Result<()> {
    let build = build
        .as_object()
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} build must be an object"))?;
    if build.get("additional_contexts").is_some() || build.get("dockerfile_inline").is_some() {
        return Err(anyhow!(
            "Docker Compose service {service_name} cannot use additional build contexts or inline Dockerfiles"
        ));
    }
    let context = build
        .get("context")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} build context is missing"))?;
    let context_path = Path::new(context);
    let context_path = if context_path.is_absolute() {
        context_path.to_path_buf()
    } else {
        project_root.join(context_path)
    };
    let context = std::fs::canonicalize(context_path.as_path())
        .with_context(|| format!("resolve Docker Compose build context {context}"))?;
    if !context.starts_with(project_root) {
        return Err(anyhow!(
            "Docker Compose service {service_name} build context escapes the authorized project"
        ));
    }
    if let Some(dockerfile) = build.get("dockerfile").and_then(Value::as_str) {
        let dockerfile = Path::new(dockerfile);
        let dockerfile = if dockerfile.is_absolute() {
            dockerfile.to_path_buf()
        } else {
            context.join(dockerfile)
        };
        let dockerfile = std::fs::canonicalize(dockerfile.as_path()).with_context(|| {
            format!("resolve Docker Compose Dockerfile {}", dockerfile.display())
        })?;
        if !dockerfile.starts_with(project_root) || !dockerfile.is_file() {
            return Err(anyhow!(
                "Docker Compose service {service_name} Dockerfile escapes the authorized project"
            ));
        }
    }
    Ok(())
}

fn validate_compose_volumes(service_name: &str, volumes: &Value) -> Result<()> {
    let volumes = volumes
        .as_array()
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} volumes must be an array"))?;
    for volume in volumes {
        let volume = volume.as_object().ok_or_else(|| {
            anyhow!("Docker Compose service {service_name} volume must be normalized")
        })?;
        if volume.get("type").and_then(Value::as_str) != Some("volume") {
            return Err(anyhow!(
                "Docker Compose service {service_name} can only use managed named volumes; host bind mounts and named pipes are forbidden"
            ));
        }
        if volume
            .get("source")
            .and_then(Value::as_str)
            .is_none_or(|source| source.trim().is_empty())
        {
            return Err(anyhow!(
                "Docker Compose service {service_name} volume source is missing"
            ));
        }
    }
    Ok(())
}

fn validate_compose_ports(service_name: &str, ports: &Value) -> Result<()> {
    let ports = ports
        .as_array()
        .ok_or_else(|| anyhow!("Docker Compose service {service_name} ports must be an array"))?;
    for port in ports {
        let port = port.as_object().ok_or_else(|| {
            anyhow!("Docker Compose service {service_name} port must be normalized")
        })?;
        let host_ip = port
            .get("host_ip")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(host_ip, "127.0.0.1" | "::1") {
            return Err(anyhow!(
                "Docker Compose service {service_name} published ports must bind to loopback"
            ));
        }
    }
    Ok(())
}

async fn run_compose_command(
    project_root: &Path,
    compose_path: &Path,
    project_name: &str,
    operation: &[&str],
) -> Result<String> {
    let mut command = docker_command();
    command
        .current_dir(project_root)
        .arg("compose")
        .arg("--project-name")
        .arg(project_name)
        .arg("--file")
        .arg(compose_path)
        .args(operation)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().await.context("execute docker compose")?;
    let stdout = String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string();
    let stderr = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    if !output.status.success() {
        return Err(anyhow!(
            "docker compose {} failed with {}: {}",
            operation.join(" "),
            output.status,
            if stderr.is_empty() { stdout } else { stderr }
        ));
    }
    Ok(if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    })
}

fn parse_compose_ps(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

async fn restrict_generated_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .await
            .with_context(|| format!("restrict generated file {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_project_path_cannot_escape_workspace() {
        assert!(normalize_relative_path("services/api").is_ok());
        assert!(normalize_relative_path("../outside").is_err());
        assert!(normalize_relative_path("/tmp/outside").is_err());
    }

    #[test]
    fn compose_rejects_host_control_settings() {
        let request = ComposeUpRequest {
            project_name: "chatos-project".to_string(),
            project_relative_path: None,
            compose_yaml: "services:\n  application:\n    privileged: true\n".to_string(),
            application_dockerfile: "FROM alpine\n".to_string(),
            env_file: String::new(),
        };
        assert!(validate_generated_content(&request).is_err());
    }

    #[test]
    fn compose_source_rejects_external_env_files_and_yaml_aliases() {
        assert!(validate_compose_source_before_resolution(
            "services:\n  application:\n    env_file: C:/Users/demo/.env\n"
        )
        .is_err());
        assert!(validate_compose_source_before_resolution(
            "services:\n  application: &base\n    image: alpine\n"
        )
        .is_err());
        assert!(validate_compose_source_before_resolution(
            "services:\n  application:\n    \"env\\u005ffile\": C:/Users/demo/.env\n"
        )
        .is_err());
        assert!(validate_compose_source_before_resolution(
            "services:\n  application:\n    env_file: [.env.chatos]\n"
        )
        .is_ok());
    }

    #[test]
    fn normalized_compose_rejects_bind_mounts_and_public_ports() {
        let project_root = std::env::temp_dir();
        let bind_mount = json!({
            "services": {
                "application": {
                    "volumes": [{
                        "type": "bind",
                        "source": "C:/",
                        "target": "/host"
                    }]
                }
            }
        });
        assert!(validate_normalized_compose(project_root.as_path(), &bind_mount).is_err());

        let public_port = json!({
            "services": {
                "application": {
                    "ports": [{
                        "target": 8080,
                        "published": "8080"
                    }]
                }
            }
        });
        assert!(validate_normalized_compose(project_root.as_path(), &public_port).is_err());
    }

    #[test]
    fn normalized_compose_accepts_named_volumes_and_loopback_ports() {
        let project_root = std::env::temp_dir();
        let compose = json!({
            "services": {
                "application": {
                    "ports": [{
                        "host_ip": "127.0.0.1",
                        "target": 8080,
                        "published": "8080"
                    }],
                    "volumes": [{
                        "type": "volume",
                        "source": "project-data",
                        "target": "/data"
                    }]
                }
            },
            "volumes": {
                "project-data": {}
            }
        });
        validate_normalized_compose(project_root.as_path(), &compose)
            .expect("safe normalized compose");
    }
}
