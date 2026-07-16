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

    run_compose_command(
        project_root.as_path(),
        compose_path.as_path(),
        project_name,
        &["config", "--quiet"],
    )
    .await
    .context("validate generated Docker Compose file")?;
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
}
