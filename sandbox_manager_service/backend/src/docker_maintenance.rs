// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::process::Output;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::{AppConfig, SandboxBackendKind};

const TEST_MAINTENANCE_ENABLED_ENV: &str = "SANDBOX_MANAGER_TEST_DOCKER_MAINTENANCE";
static DOCKER_MAINTENANCE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub async fn enforce_build_cache_limit(config: &AppConfig) -> Result<String, String> {
    if config.backend != SandboxBackendKind::Docker || !config.docker_maintenance_enabled {
        return Ok("Docker maintenance is disabled for this sandbox backend".to_string());
    }
    if cfg!(test) && std::env::var_os(TEST_MAINTENANCE_ENABLED_ENV).is_none() {
        return Ok("Docker maintenance is disabled during tests".to_string());
    }

    let _guard = maintenance_lock().lock().await;
    let dangling_images = cleanup_managed_dangling_images(config)
        .await
        .unwrap_or_else(|error| format!("managed dangling image cleanup failed: {error}"));
    bootstrap_buildkit(config).await?;
    let modern_args = [
        "builder",
        "prune",
        "--force",
        "--all",
        "--max-used-space",
        config.docker_build_cache_max_used_space.as_str(),
        "--reserved-space",
        config.docker_build_cache_reserved_space.as_str(),
    ];
    let output = run_docker_maintenance_command(config, &modern_args).await?;
    if output.status.success() {
        return Ok(format!(
            "{dangling_images}; Docker BuildKit cache limit {}: {}",
            config.docker_build_cache_max_used_space,
            output_summary(&output)
        ));
    }

    let modern_error = bounded_output(output.stderr.as_slice());
    if !unsupported_space_limit_flag(modern_error.as_str()) {
        return Err(format!("Docker BuildKit cache GC failed: {modern_error}"));
    }

    let legacy_args = [
        "builder",
        "prune",
        "--force",
        "--all",
        "--keep-storage",
        config.docker_build_cache_max_used_space.as_str(),
    ];
    let output = run_docker_maintenance_command(config, &legacy_args).await?;
    if !output.status.success() {
        return Err(format!(
            "Docker BuildKit cache GC failed with modern flags ({modern_error}) and legacy flags ({})",
            bounded_output(output.stderr.as_slice())
        ));
    }
    Ok(format!(
        "{dangling_images}; Docker BuildKit cache limit {} (legacy Docker CLI): {}",
        config.docker_build_cache_max_used_space,
        output_summary(&output)
    ))
}

async fn bootstrap_buildkit(config: &AppConfig) -> Result<(), String> {
    let output =
        run_docker_maintenance_command(config, &["buildx", "inspect", "default", "--bootstrap"])
            .await?;
    if output.status.success() {
        return Ok(());
    }
    let bootstrap_error = bounded_output(output.stderr.as_slice());
    if !unsupported_buildkit_bootstrap_flag(bootstrap_error.as_str()) {
        return Err(format!(
            "Docker BuildKit bootstrap failed: {bootstrap_error}"
        ));
    }

    let fallback_output =
        run_docker_maintenance_command(config, &["buildx", "inspect", "default"]).await?;
    if !fallback_output.status.success() {
        return Err(format!(
            "Docker BuildKit inspect failed after bootstrap fallback: {}",
            bounded_output(fallback_output.stderr.as_slice())
        ));
    }
    Ok(())
}

fn maintenance_lock() -> &'static Mutex<()> {
    DOCKER_MAINTENANCE_LOCK.get_or_init(|| Mutex::new(()))
}

async fn cleanup_managed_dangling_images(config: &AppConfig) -> Result<String, String> {
    let mut command = Command::new("docker");
    command.args([
        "image",
        "prune",
        "--force",
        "--filter",
        "label=chatos.managed=true",
    ]);
    apply_docker_connection(config, &mut command)?;
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .map_err(|_| "managed dangling image cleanup timed out after 60 seconds".to_string())?
        .map_err(|error| format!("start managed dangling image cleanup failed: {error}"))?;
    if !output.status.success() {
        return Err(bounded_output(output.stderr.as_slice()));
    }
    Ok(format!(
        "managed dangling images: {}",
        output_summary(&output)
    ))
}

async fn run_docker_maintenance_command(
    config: &AppConfig,
    args: &[&str],
) -> Result<Output, String> {
    let mut command = Command::new("docker");
    command.args(args).kill_on_drop(true);
    apply_docker_connection(config, &mut command)?;
    tokio::time::timeout(
        config
            .docker_build_cache_timeout
            .saturating_add(Duration::from_secs(30)),
        command.output(),
    )
    .await
    .map_err(|_| {
        format!(
            "Docker BuildKit cache GC timed out after {} seconds",
            config
                .docker_build_cache_timeout
                .saturating_add(Duration::from_secs(30))
                .as_secs()
        )
    })?
    .map_err(|error| format!("start Docker BuildKit cache GC failed: {error}"))
}

pub(crate) fn apply_docker_connection(
    config: &AppConfig,
    command: &mut Command,
) -> Result<(), String> {
    if let Some(docker_config) = config.docker_config.as_deref() {
        std::fs::create_dir_all(docker_config)
            .map_err(|error| format!("create Docker config directory failed: {error}"))?;
        command.env("DOCKER_CONFIG", docker_config);
    }
    if let Some(docker_host) = config.docker_host.as_deref() {
        command.env("DOCKER_HOST", docker_host);
    }
    Ok(())
}

fn unsupported_space_limit_flag(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("unknown flag")
        || error.contains("unknown shorthand")
        || error.contains("flag provided but not defined")
}

fn unsupported_buildkit_bootstrap_flag(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("--bootstrap") && unsupported_space_limit_flag(error.as_str())
}

fn output_summary(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(output.stdout.as_slice());
    let stderr = String::from_utf8_lossy(output.stderr.as_slice());
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

#[cfg(test)]
mod tests {
    use super::{unsupported_buildkit_bootstrap_flag, unsupported_space_limit_flag};

    #[test]
    fn legacy_fallback_is_only_used_for_unsupported_flags() {
        assert!(unsupported_space_limit_flag(
            "unknown flag: --max-used-space"
        ));
        assert!(!unsupported_space_limit_flag(
            "Cannot connect to the Docker daemon"
        ));
    }

    #[test]
    fn bootstrap_fallback_only_matches_unsupported_bootstrap_flag() {
        assert!(unsupported_buildkit_bootstrap_flag(
            "unknown flag: --bootstrap"
        ));
        assert!(!unsupported_buildkit_bootstrap_flag(
            "unknown flag: --max-used-space"
        ));
    }
}
