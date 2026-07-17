// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
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
const MAX_APPLICATION_SERVICES: usize = 64;

#[derive(Debug, Deserialize)]
pub(crate) struct ComposeUpRequest {
    pub(crate) project_name: String,
    #[serde(default)]
    pub(crate) project_relative_path: Option<String>,
    pub(crate) compose_yaml: String,
    #[serde(default)]
    pub(crate) application_dockerfile: Option<String>,
    #[serde(default)]
    pub(crate) application_dockerfiles: BTreeMap<String, String>,
    pub(crate) env_file: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ComposeProjectRequest {
    pub(crate) project_name: String,
    #[serde(default)]
    pub(crate) project_relative_path: Option<String>,
}

pub(crate) async fn start_project_compose_environment(
    state: &LocalState,
    workspace_id: &str,
    body: Value,
) -> Result<Value> {
    let input: ComposeUpRequest =
        serde_json::from_value(body).context("parse Docker Compose request")?;
    let project_name = validate_project_name(input.project_name.as_str())?;
    let application_dockerfiles = normalized_application_dockerfiles(&input)?;
    validate_generated_content(&input, &application_dockerfiles)?;
    ensure_docker_running().await?;

    let project_root =
        resolve_project_root(state, workspace_id, input.project_relative_path.as_deref()).await?;
    let runtime_directory = project_root.join(RUNTIME_DIRECTORY);
    tokio::fs::create_dir_all(runtime_directory.as_path())
        .await
        .with_context(|| format!("create runtime directory {}", runtime_directory.display()))?;
    let compose_path = runtime_directory.join(COMPOSE_FILE_NAME);
    let env_path = runtime_directory.join(ENV_FILE_NAME);
    tokio::fs::write(compose_path.as_path(), input.compose_yaml.as_bytes())
        .await
        .with_context(|| format!("write {}", compose_path.display()))?;
    write_application_dockerfiles(runtime_directory.as_path(), &application_dockerfiles).await?;
    if let Some(legacy_dockerfile) = input.application_dockerfile.as_deref() {
        write_legacy_application_dockerfile(runtime_directory.as_path(), legacy_dockerfile).await?;
    }
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

pub(crate) async fn get_project_compose_environment_status(
    state: &LocalState,
    workspace_id: &str,
    body: Value,
) -> Result<Value> {
    let input: ComposeProjectRequest =
        serde_json::from_value(body).context("parse Docker Compose status request")?;
    let (project_name, project_root, compose_path) =
        prepare_existing_project_compose(state, workspace_id, &input).await?;
    let ps_output = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name.as_str(),
        &["ps", "--format", "json"],
    )
    .await
    .unwrap_or_default();
    let services = parse_compose_ps(ps_output.as_str());
    Ok(json!({
        "project_name": project_name,
        "status": compose_environment_status(services.as_slice()),
        "runtime_directory": compose_path.parent().map(|path| path.to_string_lossy().to_string()),
        "compose_file": compose_path.to_string_lossy(),
        "services": services,
    }))
}

pub(crate) async fn stop_project_compose_environment(
    state: &LocalState,
    workspace_id: &str,
    body: Value,
) -> Result<Value> {
    let input: ComposeProjectRequest =
        serde_json::from_value(body).context("parse Docker Compose stop request")?;
    let (project_name, project_root, compose_path) =
        prepare_existing_project_compose(state, workspace_id, &input).await?;
    let output = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name.as_str(),
        &["down", "--remove-orphans"],
    )
    .await
    .context("stop project Docker Compose environment")?;
    Ok(json!({
        "project_name": project_name,
        "status": "stopped",
        "runtime_directory": compose_path.parent().map(|path| path.to_string_lossy().to_string()),
        "compose_file": compose_path.to_string_lossy(),
        "output": output,
        "services": [],
    }))
}

pub(crate) async fn restart_project_compose_environment(
    state: &LocalState,
    workspace_id: &str,
    body: Value,
) -> Result<Value> {
    let input: ComposeProjectRequest =
        serde_json::from_value(body).context("parse Docker Compose restart request")?;
    let (project_name, project_root, compose_path) =
        prepare_existing_project_compose(state, workspace_id, &input).await?;
    let output = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name.as_str(),
        &["up", "-d", "--build", "--remove-orphans"],
    )
    .await
    .context("restart project Docker Compose environment")?;
    let ps_output = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name.as_str(),
        &["ps", "--format", "json"],
    )
    .await
    .unwrap_or_default();
    let services = parse_compose_ps(ps_output.as_str());
    Ok(json!({
        "project_name": project_name,
        "status": compose_environment_status(services.as_slice()),
        "runtime_directory": compose_path.parent().map(|path| path.to_string_lossy().to_string()),
        "compose_file": compose_path.to_string_lossy(),
        "output": output,
        "services": services,
    }))
}

async fn prepare_existing_project_compose(
    state: &LocalState,
    workspace_id: &str,
    input: &ComposeProjectRequest,
) -> Result<(String, PathBuf, PathBuf)> {
    let project_name = validate_project_name(input.project_name.as_str())?.to_string();
    ensure_docker_running().await?;
    let project_root =
        resolve_project_root(state, workspace_id, input.project_relative_path.as_deref()).await?;
    let compose_path = project_root.join(RUNTIME_DIRECTORY).join(COMPOSE_FILE_NAME);
    let compose_source = tokio::fs::read_to_string(compose_path.as_path())
        .await
        .with_context(|| format!("read managed Compose file {}", compose_path.display()))?;
    if compose_source.is_empty() || compose_source.len() > MAX_COMPOSE_BYTES {
        return Err(anyhow!("managed Docker Compose file is empty or too large"));
    }
    validate_compose_source_before_resolution(compose_source.as_str())?;
    let normalized = run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name.as_str(),
        &["config", "--format", "json"],
    )
    .await
    .context("validate managed Docker Compose file")?;
    let normalized = serde_json::from_str::<Value>(normalized.as_str())
        .context("parse managed Docker Compose configuration")?;
    validate_normalized_compose(project_root.as_path(), &normalized)?;
    Ok((project_name, project_root, compose_path))
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

fn normalized_application_dockerfiles(
    input: &ComposeUpRequest,
) -> Result<BTreeMap<String, String>> {
    let mut dockerfiles = input.application_dockerfiles.clone();
    if dockerfiles.is_empty() {
        if let Some(dockerfile) = input.application_dockerfile.as_deref() {
            dockerfiles.insert("application".to_string(), dockerfile.to_string());
        }
    }
    if dockerfiles.is_empty() || dockerfiles.len() > MAX_APPLICATION_SERVICES {
        return Err(anyhow!(
            "generated environment must contain between 1 and {MAX_APPLICATION_SERVICES} application Dockerfiles"
        ));
    }
    for (service_id, dockerfile) in &dockerfiles {
        validate_application_service_id(service_id)?;
        if dockerfile.is_empty() || dockerfile.len() > MAX_DOCKERFILE_BYTES {
            return Err(anyhow!(
                "generated application Dockerfile is empty or too large: {service_id}"
            ));
        }
        if dockerfile_contains_program_managed_mcp_control(dockerfile) {
            return Err(anyhow!(
                "application Dockerfile cannot install or configure the program-managed Chat OS MCP Agent: {service_id}"
            ));
        }
    }
    Ok(dockerfiles)
}

fn validate_application_service_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 63
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        || !value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        || !value
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        return Err(anyhow!("invalid application service id: {value}"));
    }
    Ok(())
}

fn dockerfile_contains_program_managed_mcp_control(dockerfile: &str) -> bool {
    let dockerfile = dockerfile.to_ascii_lowercase();
    [
        "chatos-sandbox-mcp",
        "chatos_sandbox_mcp",
        "chat os mcp agent",
        "chatos mcp agent",
        "mcp_token",
        "mcp_port",
        "mcp_image",
        "mcp_command",
        "agent_install_script",
        "agent_injection_mode",
        "/opt/chatos/",
    ]
    .iter()
    .any(|marker| dockerfile.contains(marker))
}

async fn write_application_dockerfiles(
    runtime_directory: &Path,
    dockerfiles: &BTreeMap<String, String>,
) -> Result<()> {
    let services_directory = runtime_directory.join("services");
    if tokio::fs::try_exists(services_directory.as_path())
        .await
        .unwrap_or(false)
    {
        tokio::fs::remove_dir_all(services_directory.as_path())
            .await
            .with_context(|| format!("clean {}", services_directory.display()))?;
    }
    for (service_id, content) in dockerfiles {
        let service_directory = services_directory.join(service_id);
        tokio::fs::create_dir_all(service_directory.as_path())
            .await
            .with_context(|| format!("create {}", service_directory.display()))?;
        let dockerfile_path = service_directory.join("Dockerfile");
        let dockerignore_path = service_directory.join("Dockerfile.dockerignore");
        tokio::fs::write(dockerfile_path.as_path(), content.as_bytes())
            .await
            .with_context(|| format!("write {}", dockerfile_path.display()))?;
        tokio::fs::write(
            dockerignore_path.as_path(),
            b".chatos/runtime-environment/.env.chatos\n",
        )
        .await
        .with_context(|| format!("write {}", dockerignore_path.display()))?;
    }
    Ok(())
}

async fn write_legacy_application_dockerfile(
    runtime_directory: &Path,
    content: &str,
) -> Result<()> {
    let dockerfile_path = runtime_directory.join(APPLICATION_DOCKERFILE_NAME);
    let dockerignore_path = runtime_directory.join(APPLICATION_DOCKERIGNORE_NAME);
    tokio::fs::write(dockerfile_path.as_path(), content.as_bytes())
        .await
        .with_context(|| format!("write {}", dockerfile_path.display()))?;
    tokio::fs::write(
        dockerignore_path.as_path(),
        b".chatos/runtime-environment/.env.chatos\n",
    )
    .await
    .with_context(|| format!("write {}", dockerignore_path.display()))?;
    Ok(())
}

fn validate_generated_content(
    input: &ComposeUpRequest,
    application_dockerfiles: &BTreeMap<String, String>,
) -> Result<()> {
    if input.compose_yaml.is_empty() || input.compose_yaml.len() > MAX_COMPOSE_BYTES {
        return Err(anyhow!(
            "generated Docker Compose file is empty or too large"
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
    for marker in [
        "chatos-sandbox-mcp",
        "chatos_sandbox_mcp",
        "mcp_token",
        "mcp_port",
        "agent_install_script",
        "agent_injection_mode",
    ] {
        if compose_lower.contains(marker) {
            return Err(anyhow!(
                "generated Docker Compose file cannot install or configure the program-managed Chat OS MCP Agent: {marker}"
            ));
        }
    }
    if !compose_lower.contains("services:") {
        return Err(anyhow!(
            "generated Docker Compose file must contain services"
        ));
    }
    if input.application_dockerfiles.is_empty() {
        if !compose_lower.contains("dockerfile.application") {
            return Err(anyhow!(
                "legacy generated Docker Compose file must reference Dockerfile.application"
            ));
        }
    } else {
        for service_id in application_dockerfiles.keys() {
            let expected = format!(".chatos/runtime-environment/services/{service_id}/dockerfile");
            if !compose_lower.contains(expected.as_str()) {
                return Err(anyhow!(
                    "generated Docker Compose file does not reference the managed Dockerfile for application service {service_id}"
                ));
            }
        }
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
    let mut application_build_count = 0usize;
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
            application_build_count += 1;
        }
        if let Some(volumes) = service.get("volumes") {
            validate_compose_volumes(service_name, volumes)?;
        }
        if let Some(ports) = service.get("ports") {
            validate_compose_ports(service_name, ports)?;
        }
    }
    if application_build_count == 0 {
        return Err(anyhow!(
            "normalized Docker Compose configuration has no managed application build service"
        ));
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
        let managed_service_dockerfile = std::fs::canonicalize(
            project_root
                .join(RUNTIME_DIRECTORY)
                .join("services")
                .join(service_name)
                .join("Dockerfile"),
        )
        .ok();
        let legacy_dockerfile = std::fs::canonicalize(
            project_root
                .join(RUNTIME_DIRECTORY)
                .join(APPLICATION_DOCKERFILE_NAME),
        )
        .ok();
        let managed = managed_service_dockerfile.as_deref() == Some(dockerfile.as_path())
            || (service_name == "application"
                && legacy_dockerfile.as_deref() == Some(dockerfile.as_path()));
        if !dockerfile.starts_with(project_root) || !dockerfile.is_file() || !managed {
            return Err(anyhow!(
                "Docker Compose service {service_name} Dockerfile is not a program-managed runtime artifact"
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
    if let Ok(Value::Array(services)) = serde_json::from_str::<Value>(output.trim()) {
        return services;
    }
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn compose_environment_status(services: &[Value]) -> &'static str {
    if services.is_empty() {
        return "stopped";
    }
    let running = services
        .iter()
        .filter(|service| {
            service
                .get("State")
                .or_else(|| service.get("state"))
                .and_then(Value::as_str)
                .is_some_and(|state| state.eq_ignore_ascii_case("running"))
        })
        .count();
    if running == services.len() {
        "running"
    } else if running == 0 {
        "stopped"
    } else {
        "degraded"
    }
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
            application_dockerfile: Some("FROM alpine\n".to_string()),
            application_dockerfiles: BTreeMap::new(),
            env_file: String::new(),
        };
        let dockerfiles = normalized_application_dockerfiles(&request).expect("dockerfiles");
        assert!(validate_generated_content(&request, &dockerfiles).is_err());
    }

    #[test]
    fn compose_accepts_multiple_program_managed_application_dockerfiles() {
        let application_dockerfiles = BTreeMap::from([
            ("api".to_string(), "FROM node:24\n".to_string()),
            ("worker".to_string(), "FROM python:3.12\n".to_string()),
        ]);
        let request = ComposeUpRequest {
            project_name: "chatos-project".to_string(),
            project_relative_path: None,
            compose_yaml: concat!(
                "services:\n",
                "  api:\n",
                "    build:\n",
                "      context: ../..\n",
                "      dockerfile: .chatos/runtime-environment/services/api/Dockerfile\n",
                "  worker:\n",
                "    build:\n",
                "      context: ../..\n",
                "      dockerfile: .chatos/runtime-environment/services/worker/Dockerfile\n",
            )
            .to_string(),
            application_dockerfile: None,
            application_dockerfiles,
            env_file: String::new(),
        };
        let dockerfiles = normalized_application_dockerfiles(&request).expect("dockerfiles");
        validate_generated_content(&request, &dockerfiles).expect("multi-app compose source");
        assert_eq!(dockerfiles.len(), 2);
    }

    #[test]
    fn compose_rejects_ai_authored_mcp_installation_in_dockerfile() {
        let request = ComposeUpRequest {
            project_name: "chatos-project".to_string(),
            project_relative_path: None,
            compose_yaml: concat!(
                "services:\n",
                "  api:\n",
                "    build:\n",
                "      context: ../..\n",
                "      dockerfile: .chatos/runtime-environment/services/api/Dockerfile\n",
            )
            .to_string(),
            application_dockerfile: None,
            application_dockerfiles: BTreeMap::from([(
                "api".to_string(),
                "FROM node:24\nCOPY chatos-sandbox-mcp-server /opt/chatos/bin/\n".to_string(),
            )]),
            env_file: String::new(),
        };
        assert!(normalized_application_dockerfiles(&request).is_err());
    }

    #[tokio::test]
    async fn writes_each_application_dockerfile_to_managed_service_directory() {
        let runtime_directory = std::env::temp_dir().join(format!(
            "chatos-compose-artifacts-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let dockerfiles = BTreeMap::from([
            ("api".to_string(), "FROM node:24\n".to_string()),
            ("worker".to_string(), "FROM python:3.12\n".to_string()),
        ]);
        write_application_dockerfiles(runtime_directory.as_path(), &dockerfiles)
            .await
            .expect("write application Dockerfiles");
        assert_eq!(
            std::fs::read_to_string(
                runtime_directory
                    .join("services")
                    .join("api")
                    .join("Dockerfile")
            )
            .expect("read api Dockerfile"),
            "FROM node:24\n"
        );
        assert_eq!(
            std::fs::read_to_string(
                runtime_directory
                    .join("services")
                    .join("worker")
                    .join("Dockerfile")
            )
            .expect("read worker Dockerfile"),
            "FROM python:3.12\n"
        );
        let _ = tokio::fs::remove_dir_all(runtime_directory).await;
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
        let project_root = std::env::temp_dir().join(format!(
            "chatos-compose-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let dockerfile = project_root
            .join(RUNTIME_DIRECTORY)
            .join("services")
            .join("application")
            .join("Dockerfile");
        std::fs::create_dir_all(dockerfile.parent().expect("dockerfile parent"))
            .expect("create test runtime directory");
        std::fs::write(dockerfile.as_path(), "FROM alpine\n").expect("write test Dockerfile");
        let project_root = std::fs::canonicalize(project_root).expect("canonical project root");
        let build_context = project_root.to_string_lossy().to_string();
        let compose = json!({
            "services": {
                "application": {
                    "build": {
                        "context": build_context,
                        "dockerfile": ".chatos/runtime-environment/services/application/Dockerfile"
                    },
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
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn normalized_compose_service_cannot_reuse_another_services_dockerfile() {
        let project_root = std::env::temp_dir().join(format!(
            "chatos-compose-service-binding-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let api_dockerfile = project_root
            .join(RUNTIME_DIRECTORY)
            .join("services")
            .join("api")
            .join("Dockerfile");
        std::fs::create_dir_all(api_dockerfile.parent().expect("api Dockerfile parent"))
            .expect("create managed API directory");
        std::fs::write(api_dockerfile.as_path(), "FROM alpine\n")
            .expect("write managed API Dockerfile");
        let project_root = std::fs::canonicalize(project_root).expect("canonical project root");
        let compose = json!({
            "services": {
                "worker": {
                    "build": {
                        "context": project_root.to_string_lossy(),
                        "dockerfile": ".chatos/runtime-environment/services/api/Dockerfile"
                    }
                }
            }
        });
        assert!(validate_normalized_compose(project_root.as_path(), &compose).is_err());
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn compose_parent_status_is_derived_from_all_child_services() {
        assert_eq!(compose_environment_status(&[]), "stopped");
        assert_eq!(
            compose_environment_status(&[json!({"State": "running"}), json!({"State": "running"})]),
            "running"
        );
        assert_eq!(
            compose_environment_status(&[json!({"State": "running"}), json!({"State": "exited"})]),
            "degraded"
        );
        assert_eq!(
            compose_environment_status(&[json!({"State": "exited"})]),
            "stopped"
        );
        assert_eq!(
            parse_compose_ps(r#"[{"State":"running"},{"State":"exited"}]"#).len(),
            2
        );
    }
}
