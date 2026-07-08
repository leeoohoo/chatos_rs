// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::sandbox::types::{LocalSandboxNetworkPolicy, LocalSandboxResourceLimits};

use super::DEFAULT_LOCAL_SANDBOX_AGENT_PORT;

pub(crate) async fn start_local_sandbox_container(
    sandbox_id: &str,
    run_workspace: &Path,
    image_ref: &str,
    agent_token: &str,
    resource_limits: &LocalSandboxResourceLimits,
    network: &LocalSandboxNetworkPolicy,
) -> Result<String> {
    let name = local_sandbox_container_name(sandbox_id);
    let network_mode = if network.mode.trim().is_empty() {
        "bridge"
    } else {
        network.mode.trim()
    };
    let mut command = tokio::process::Command::new("docker");
    command
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(name.as_str())
        .arg("--hostname")
        .arg(name.as_str())
        .arg("--label")
        .arg(format!("chatos.local_connector.sandbox_id={sandbox_id}"))
        .arg("--network")
        .arg(network_mode)
        .arg("--cpus")
        .arg(resource_limits.cpu.max(0.1).to_string())
        .arg("--memory")
        .arg(format!("{}m", resource_limits.memory_mb.max(128)))
        .arg("--pids-limit")
        .arg(resource_limits.max_processes.max(16).to_string())
        .arg("--workdir")
        .arg("/workspace")
        .arg("-e")
        .arg(format!("CHATOS_SANDBOX_ID={sandbox_id}"))
        .arg("-e")
        .arg(format!("CHATOS_SANDBOX_MCP_TOKEN={agent_token}"));
    if network_mode != "none" {
        command
            .arg("-p")
            .arg(format!("127.0.0.1::{DEFAULT_LOCAL_SANDBOX_AGENT_PORT}"));
    }
    command
        .arg("--tmpfs")
        .arg("/tmp:rw,nosuid,size=512m")
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("-v")
        .arg(format!("{}:/workspace:rw", run_workspace.display()))
        .arg(image_ref);
    let output = command
        .output()
        .await
        .context("start local docker sandbox")?;
    if !output.status.success() {
        return Err(anyhow!(
            "docker run failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ));
    }
    Ok(String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string())
}

pub(crate) async fn inspect_local_sandbox_container(sandbox_id: &str) -> Result<bool> {
    let output = tokio::process::Command::new("docker")
        .arg("inspect")
        .arg("-f")
        .arg("{{.State.Running}}")
        .arg(local_sandbox_container_name(sandbox_id))
        .output()
        .await
        .context("inspect local sandbox container")?;
    Ok(output.status.success()
        && String::from_utf8_lossy(output.stdout.as_slice())
            .trim()
            .eq_ignore_ascii_case("true"))
}

pub(crate) async fn destroy_local_sandbox_container(sandbox_id: &str) -> Result<()> {
    let output = tokio::process::Command::new("docker")
        .arg("rm")
        .arg("-f")
        .arg(local_sandbox_container_name(sandbox_id))
        .output()
        .await
        .context("remove local sandbox container")?;
    if output.status.success()
        || String::from_utf8_lossy(output.stderr.as_slice()).contains("No such container")
    {
        Ok(())
    } else {
        Err(anyhow!(
            "docker rm failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ))
    }
}

pub(crate) async fn published_local_sandbox_agent_endpoint(sandbox_id: &str) -> Option<String> {
    let output = tokio::process::Command::new("docker")
        .arg("port")
        .arg(local_sandbox_container_name(sandbox_id))
        .arg(format!("{DEFAULT_LOCAL_SANDBOX_AGENT_PORT}/tcp"))
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(output.stdout.as_slice());
    let line = stdout.lines().next()?.trim();
    let host_port = line.rsplit(':').next()?.trim();
    if host_port.is_empty() {
        None
    } else {
        Some(format!("http://127.0.0.1:{host_port}"))
    }
}

pub(crate) async fn wait_for_local_sandbox_agent(
    http_client: &reqwest::Client,
    agent_endpoint: &str,
) -> Result<()> {
    let health_url = format!("{}/health", agent_endpoint.trim_end_matches('/'));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let last_error = match http_client
            .get(health_url.as_str())
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => format!("HTTP {}", response.status()),
            Err(err) => err.to_string(),
        };
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "local sandbox agent did not become healthy at {agent_endpoint}: {last_error}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn local_sandbox_container_name(sandbox_id: &str) -> String {
    format!("chatos-local-{sandbox_id}")
}
