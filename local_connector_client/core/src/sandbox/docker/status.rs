// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Serialize)]
pub(crate) struct DockerStatusLocal {
    pub(super) installed: bool,
    pub(super) running: bool,
    pub(super) version: Option<String>,
    pub(super) error: Option<String>,
}

pub(crate) async fn docker_status() -> Value {
    json!(docker_status_struct().await)
}

pub(crate) async fn docker_status_struct() -> DockerStatusLocal {
    let version = run_command_capture("docker", &["--version"], Duration::from_secs(5)).await;
    let Ok(version) = version else {
        return DockerStatusLocal {
            installed: false,
            running: false,
            version: None,
            error: Some("docker command is not available".to_string()),
        };
    };
    let version_text = first_non_empty_line(version.1.as_str())
        .or_else(|| first_non_empty_line(version.2.as_str()));
    let info = run_command_capture("docker", &["info"], Duration::from_secs(5)).await;
    match info {
        Ok((code, _, _)) if code == 0 => DockerStatusLocal {
            installed: true,
            running: true,
            version: version_text,
            error: None,
        },
        Ok((_, _, stderr)) => DockerStatusLocal {
            installed: true,
            running: false,
            version: version_text,
            error: non_empty_trimmed(Some(stderr.as_str())),
        },
        Err(err) => DockerStatusLocal {
            installed: true,
            running: false,
            version: version_text,
            error: Some(err.to_string()),
        },
    }
}

pub(crate) async fn ensure_docker_running() -> Result<()> {
    let status = docker_status_struct().await;
    if !status.installed {
        return Err(anyhow!(
            "Docker is not installed or docker command is not in PATH"
        ));
    }
    if status.running {
        return Ok(());
    }
    start_docker_desktop().await?;
    let started_at = std::time::Instant::now();
    while started_at.elapsed() < Duration::from_secs(60) {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if docker_status_struct().await.running {
            return Ok(());
        }
    }
    Err(anyhow!("Docker did not become ready within 60 seconds"))
}

async fn start_docker_desktop() -> Result<()> {
    match std::env::consts::OS {
        "macos" => {
            let _ = tokio::process::Command::new("open")
                .args(["-a", "Docker"])
                .status()
                .await
                .context("start Docker Desktop")?;
        }
        "windows" => {
            let _ = tokio::process::Command::new("cmd")
                .args(["/C", "start", "", "Docker Desktop"])
                .status()
                .await
                .context("start Docker Desktop")?;
        }
        _ => {
            let _ = tokio::process::Command::new("systemctl")
                .args(["--user", "start", "docker"])
                .status()
                .await;
        }
    }
    Ok(())
}

async fn run_command_capture(
    program: &str,
    args: &[&str],
    timeout_duration: Duration,
) -> Result<(i32, String, String)> {
    let output = tokio::time::timeout(
        timeout_duration,
        tokio::process::Command::new(program).args(args).output(),
    )
    .await
    .with_context(|| format!("{program} timed out"))?
    .with_context(|| format!("run {program}"))?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(output.stdout.as_slice()).into_owned(),
        String::from_utf8_lossy(output.stderr.as_slice()).into_owned(),
    ))
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn non_empty_trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
