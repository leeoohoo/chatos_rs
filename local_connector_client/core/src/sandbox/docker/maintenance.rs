// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::config::optional_env;

use super::docker_command;

const MAINTENANCE_ENABLED_ENV: &str = "LOCAL_CONNECTOR_DOCKER_MAINTENANCE_ENABLED";
const BUILD_CACHE_MAX_USED_SPACE_ENV: &str = "LOCAL_CONNECTOR_DOCKER_BUILD_CACHE_MAX_USED_SPACE";
const BUILD_CACHE_RESERVED_SPACE_ENV: &str = "LOCAL_CONNECTOR_DOCKER_BUILD_CACHE_RESERVED_SPACE";
const BUILD_CACHE_TIMEOUT_SECS_ENV: &str = "LOCAL_CONNECTOR_DOCKER_BUILD_CACHE_TIMEOUT_SECS";
const TEST_MAINTENANCE_ENABLED_ENV: &str = "LOCAL_CONNECTOR_TEST_DOCKER_MAINTENANCE";
const DEFAULT_BUILD_CACHE_MAX_USED_SPACE: &str = "32gb";
const DEFAULT_BUILD_CACHE_RESERVED_SPACE: &str = "8gb";
const DEFAULT_BUILD_CACHE_TIMEOUT_SECS: u64 = 180;

static DOCKER_MAINTENANCE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) async fn maintain_workspace_docker_artifacts(workspace_root: &Path) -> Value {
    if !docker_maintenance_enabled() {
        return json!({
            "enabled": false,
            "skipped": "local Docker maintenance is disabled"
        });
    }

    let _guard = maintenance_lock().lock().await;
    let workspace_root = canonical_or_original(workspace_root);
    let (compose_projects, project_discovery_error) =
        match compose_projects_for_workspace(workspace_root.as_path()).await {
            Ok(projects) => (projects, None),
            Err(error) => (Vec::new(), Some(error)),
        };
    let dangling_images = cleanup_compose_dangling_images(compose_projects.as_slice()).await;
    let build_cache = enforce_docker_build_cache_limit_locked().await;

    json!({
        "enabled": true,
        "workspace_root": workspace_root.to_string_lossy(),
        "compose_projects": compose_projects,
        "project_discovery_error": project_discovery_error,
        "dangling_images": dangling_images,
        "build_cache": build_cache,
    })
}

pub(crate) async fn enforce_docker_build_cache_limit() -> Value {
    if !docker_maintenance_enabled() {
        return json!({
            "enabled": false,
            "skipped": "local Docker maintenance is disabled"
        });
    }
    let _guard = maintenance_lock().lock().await;
    enforce_docker_build_cache_limit_locked().await
}

pub(crate) fn docker_maintenance_report_message(report: &Value) -> String {
    if !report
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return "Docker maintenance disabled".to_string();
    }
    if let Some(error) = report.get("error").and_then(Value::as_str) {
        return format!("Docker maintenance failed: {error}");
    }
    if let Some(build_cache) = report.get("build_cache") {
        return docker_maintenance_report_message(build_cache);
    }
    let limit = report
        .get("max_used_space")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_BUILD_CACHE_MAX_USED_SPACE);
    let summary = report
        .get("summary")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("cache is within the configured limit");
    format!("Docker BuildKit cache limit {limit}: {summary}")
}

fn maintenance_lock() -> &'static Mutex<()> {
    DOCKER_MAINTENANCE_LOCK.get_or_init(|| Mutex::new(()))
}

fn docker_maintenance_enabled() -> bool {
    if cfg!(test) && optional_env(TEST_MAINTENANCE_ENABLED_ENV).is_none() {
        return false;
    }
    optional_env(MAINTENANCE_ENABLED_ENV)
        .map(|value| env_flag_enabled(value.as_str()))
        .unwrap_or(true)
}

fn env_flag_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

async fn compose_projects_for_workspace(workspace_root: &Path) -> Result<Vec<String>, String> {
    let output = docker_command()
        .args([
            "ps",
            "--all",
            "--quiet",
            "--filter",
            "label=com.docker.compose.project",
        ])
        .output()
        .await
        .map_err(|error| format!("list Docker Compose containers failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "list Docker Compose containers failed: {}",
            bounded_output(output.stderr.as_slice())
        ));
    }
    let container_ids = String::from_utf8_lossy(output.stdout.as_slice())
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if container_ids.is_empty() {
        return Ok(Vec::new());
    }

    let output = docker_command()
        .arg("inspect")
        .args(container_ids)
        .output()
        .await
        .map_err(|error| format!("inspect Docker Compose containers failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "inspect Docker Compose containers failed: {}",
            bounded_output(output.stderr.as_slice())
        ));
    }
    let inspected = serde_json::from_slice::<Value>(output.stdout.as_slice())
        .map_err(|error| format!("parse Docker Compose container inspection failed: {error}"))?;
    Ok(compose_projects_from_inspection(&inspected, workspace_root))
}

fn compose_projects_from_inspection(inspected: &Value, workspace_root: &Path) -> Vec<String> {
    let mut projects = BTreeSet::new();
    for container in inspected.as_array().into_iter().flatten() {
        let Some(labels) = container
            .pointer("/Config/Labels")
            .and_then(Value::as_object)
        else {
            continue;
        };
        let Some(project_name) = labels
            .get("com.docker.compose.project")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if compose_labels_belong_to_workspace(labels, workspace_root) {
            projects.insert(project_name.to_string());
        }
    }
    projects.into_iter().collect()
}

fn compose_labels_belong_to_workspace(
    labels: &serde_json::Map<String, Value>,
    workspace_root: &Path,
) -> bool {
    [
        "com.docker.compose.project.working_dir",
        "com.docker.compose.project.config_files",
        "com.docker.compose.project.environment_file",
    ]
    .into_iter()
    .filter_map(|key| labels.get(key).and_then(Value::as_str))
    .flat_map(|value| value.split(','))
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(Path::new)
    .any(|path| path_belongs_to_workspace(path, workspace_root))
}

fn path_belongs_to_workspace(path: &Path, workspace_root: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    canonical_or_original(path).starts_with(workspace_root)
}

async fn cleanup_compose_dangling_images(compose_projects: &[String]) -> Value {
    let mut image_ids = BTreeSet::new();
    let mut errors = Vec::new();
    for project_name in compose_projects {
        let filter = format!("label=com.docker.compose.project={project_name}");
        let output = docker_command()
            .args([
                "image",
                "ls",
                "--all",
                "--quiet",
                "--no-trunc",
                "--filter",
                "dangling=true",
                "--filter",
                filter.as_str(),
            ])
            .output()
            .await;
        match output {
            Ok(output) if output.status.success() => {
                image_ids.extend(
                    String::from_utf8_lossy(output.stdout.as_slice())
                        .lines()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned),
                );
            }
            Ok(output) => errors.push(format!(
                "list dangling images for Compose project {project_name} failed: {}",
                bounded_output(output.stderr.as_slice())
            )),
            Err(error) => errors.push(format!(
                "list dangling images for Compose project {project_name} failed: {error}"
            )),
        }
    }

    let found = image_ids.len();
    let mut removed = Vec::new();
    let mut retained = Vec::new();
    for image_id in image_ids {
        match docker_command()
            .args(["image", "rm", image_id.as_str()])
            .output()
            .await
        {
            Ok(output) if output.status.success() => removed.push(image_id),
            Ok(output) => retained.push(json!({
                "image_id": image_id,
                "reason": bounded_output(output.stderr.as_slice()),
            })),
            Err(error) => retained.push(json!({
                "image_id": image_id,
                "reason": error.to_string(),
            })),
        }
    }

    json!({
        "found": found,
        "removed": removed,
        "retained": retained,
        "errors": errors,
    })
}

async fn enforce_docker_build_cache_limit_locked() -> Value {
    let max_used_space = configured_storage_limit(
        BUILD_CACHE_MAX_USED_SPACE_ENV,
        DEFAULT_BUILD_CACHE_MAX_USED_SPACE,
    );
    let reserved_space = configured_storage_limit(
        BUILD_CACHE_RESERVED_SPACE_ENV,
        DEFAULT_BUILD_CACHE_RESERVED_SPACE,
    );
    let timeout_secs = optional_env(BUILD_CACHE_TIMEOUT_SECS_ENV)
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_BUILD_CACHE_TIMEOUT_SECS);
    let cli_timeout = format!("{timeout_secs}s");
    let command = docker_command()
        .args([
            "builder",
            "prune",
            "--force",
            "--all",
            "--max-used-space",
            max_used_space.as_str(),
            "--reserved-space",
            reserved_space.as_str(),
            "--timeout",
            cli_timeout.as_str(),
        ])
        .output();
    let output = match tokio::time::timeout(
        Duration::from_secs(timeout_secs.saturating_add(30)),
        command,
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return json!({
                "enabled": true,
                "max_used_space": max_used_space,
                "reserved_space": reserved_space,
                "error": format!("start Docker BuildKit cache GC failed: {error}"),
            });
        }
        Err(_) => {
            return json!({
                "enabled": true,
                "max_used_space": max_used_space,
                "reserved_space": reserved_space,
                "error": format!("Docker BuildKit cache GC timed out after {} seconds", timeout_secs.saturating_add(30)),
            });
        }
    };
    if !output.status.success() {
        return json!({
            "enabled": true,
            "max_used_space": max_used_space,
            "reserved_space": reserved_space,
            "error": bounded_output(output.stderr.as_slice()),
        });
    }
    json!({
        "enabled": true,
        "max_used_space": max_used_space,
        "reserved_space": reserved_space,
        "summary": command_output_summary(output.stdout.as_slice(), output.stderr.as_slice()),
    })
}

fn configured_storage_limit(key: &str, default: &str) -> String {
    optional_env(key)
        .filter(|value| valid_storage_limit(value.as_str()))
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| default.to_string())
}

fn valid_storage_limit(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    let digits_end = value
        .char_indices()
        .find_map(|(index, character)| (!character.is_ascii_digit()).then_some(index))
        .unwrap_or(value.len());
    let (digits, suffix) = value.split_at(digits_end);
    !digits.is_empty()
        && digits.parse::<u64>().is_ok_and(|value| value > 0)
        && matches!(suffix, "" | "b" | "kb" | "mb" | "gb" | "tb")
}

fn command_output_summary(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .last()
        .map(bounded_text)
        .unwrap_or_else(|| "cache is within the configured limit".to_string())
}

fn bounded_output(bytes: &[u8]) -> String {
    bounded_text(String::from_utf8_lossy(bytes).trim())
}

fn bounded_text(value: &str) -> String {
    value.chars().take(512).collect()
}

fn canonical_or_original(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{compose_projects_from_inspection, valid_storage_limit};

    #[test]
    fn compose_project_discovery_is_scoped_to_workspace_labels() {
        let workspace = std::env::temp_dir().join("chatos-docker-maintenance-workspace");
        let outside = std::env::temp_dir().join("chatos-docker-maintenance-outside");
        let inspected = json!([
            {
                "Config": {"Labels": {
                    "com.docker.compose.project": "inside-project",
                    "com.docker.compose.project.working_dir": workspace.join("deploy").to_string_lossy()
                }}
            },
            {
                "Config": {"Labels": {
                    "com.docker.compose.project": "outside-project",
                    "com.docker.compose.project.config_files": outside.join("docker-compose.yml").to_string_lossy()
                }}
            }
        ]);

        assert_eq!(
            compose_projects_from_inspection(&inspected, workspace.as_path()),
            vec!["inside-project".to_string()]
        );
    }

    #[test]
    fn docker_storage_limits_are_strict_and_shell_free() {
        assert!(valid_storage_limit("32gb"));
        assert!(valid_storage_limit("8192MB"));
        assert!(!valid_storage_limit("0gb"));
        assert!(!valid_storage_limit("32 gb"));
        assert!(!valid_storage_limit("32gb; rm -rf /"));
    }
}
