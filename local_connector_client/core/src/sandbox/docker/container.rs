// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::PermissionProfileId;

use crate::sandbox::types::{LocalSandboxNetworkPolicy, LocalSandboxResourceLimits};

use super::{docker_command, DEFAULT_LOCAL_SANDBOX_AGENT_PORT};

pub(crate) async fn start_local_sandbox_container(
    sandbox_id: &str,
    run_workspace: &Path,
    image_ref: &str,
    agent_token: &str,
    resource_limits: &LocalSandboxResourceLimits,
    network: &LocalSandboxNetworkPolicy,
    permission_profile: PermissionProfileId,
) -> Result<String> {
    let name = local_sandbox_container_name(sandbox_id);
    let network_mode = if network.mode.trim().is_empty() {
        "bridge"
    } else {
        network.mode.trim()
    };
    let workspace_mount_mode = workspace_mount_mode(permission_profile);
    let tmpfs_size_mb = (resource_limits.disk_mb / 16).clamp(16, 512);
    let home_tmpfs_size_mb = tmpfs_size_mb;
    let workspace_limit_mb = resource_limits
        .disk_mb
        .saturating_sub(tmpfs_size_mb.saturating_add(home_tmpfs_size_mb))
        .max(1);
    let disk_limit_bytes = workspace_limit_mb.saturating_mul(1024 * 1024);
    let sandbox_user = sandbox_user_for_workspace(run_workspace);
    let mut command = docker_command();
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
        .arg(format!("CHATOS_SANDBOX_MCP_TOKEN={agent_token}"))
        .arg("-e")
        .arg(format!(
            "CHATOS_SANDBOX_PERMISSION_PROFILE={}",
            permission_profile.as_str()
        ))
        .arg("-e")
        .arg(format!(
            "CHATOS_SANDBOX_DISK_LIMIT_BYTES={disk_limit_bytes}"
        ))
        .arg("-e")
        .arg("HOME=/home/sandbox")
        .arg("-e")
        .arg("XDG_CACHE_HOME=/home/sandbox/.cache");
    if network_mode != "none" {
        command
            .arg("-p")
            .arg(format!("127.0.0.1::{DEFAULT_LOCAL_SANDBOX_AGENT_PORT}"));
    }
    command
        .arg("--read-only")
        .arg("--cap-drop")
        .arg("ALL")
        .arg("--user")
        .arg(sandbox_user.spec.as_str())
        .arg("--tmpfs")
        .arg(format!(
            "/tmp:rw,nosuid,nodev,size={tmpfs_size_mb}m,mode=1777"
        ))
        .arg("--tmpfs")
        .arg(format!(
            "/home/sandbox:rw,nosuid,nodev,size={home_tmpfs_size_mb}m,uid={},gid={},mode=0700",
            sandbox_user.uid, sandbox_user.gid
        ))
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("-v")
        .arg(format!(
            "{}:/workspace:{workspace_mount_mode}",
            run_workspace.display()
        ))
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
    let output = docker_command()
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
    let output = docker_command()
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
    let output = docker_command()
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

fn workspace_mount_mode(permission_profile: PermissionProfileId) -> &'static str {
    match permission_profile {
        PermissionProfileId::ReadOnly => "ro",
        PermissionProfileId::WorkspaceWrite | PermissionProfileId::FullAccess => "rw",
    }
}

struct SandboxUser {
    spec: String,
    uid: u32,
    gid: u32,
}

#[cfg(unix)]
fn sandbox_user_for_workspace(workspace: &Path) -> SandboxUser {
    use std::os::unix::fs::MetadataExt;

    let (uid, gid) = workspace
        .metadata()
        .map(|metadata| (metadata.uid(), metadata.gid()))
        .unwrap_or((1000, 1000));
    SandboxUser {
        spec: format!("{uid}:{gid}"),
        uid,
        gid,
    }
}

#[cfg(not(unix))]
fn sandbox_user_for_workspace(_workspace: &Path) -> SandboxUser {
    SandboxUser {
        spec: "1000:1000".to_string(),
        uid: 1000,
        gid: 1000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_permission_mounts_workspace_read_only() {
        assert_eq!(workspace_mount_mode(PermissionProfileId::ReadOnly), "ro");
        assert_eq!(
            workspace_mount_mode(PermissionProfileId::WorkspaceWrite),
            "rw"
        );
        assert_eq!(workspace_mount_mode(PermissionProfileId::FullAccess), "rw");
    }

    #[test]
    fn sandbox_user_matches_workspace_owner_on_unix() {
        let workspace =
            std::env::temp_dir().join(format!("chatos-sandbox-user-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(workspace.as_path()).expect("workspace");
        let user = sandbox_user_for_workspace(workspace.as_path());
        assert_eq!(user.spec, format!("{}:{}", user.uid, user.gid));
        let _ = std::fs::remove_dir_all(workspace);
    }
}
