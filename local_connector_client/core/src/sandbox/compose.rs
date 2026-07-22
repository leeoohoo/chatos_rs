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

mod validation;

use self::validation::{
    validate_compose_source_before_resolution, validate_generated_content,
    validate_normalized_compose,
};

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
include!("compose.test.rs");
