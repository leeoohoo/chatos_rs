// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::PermissionProfileId;

use crate::sandbox::types::{LocalSandboxNetworkPolicy, LocalSandboxResourceLimits};

use super::{docker_command, DEFAULT_LOCAL_SANDBOX_AGENT_PORT};

const LOCAL_SANDBOX_LABEL: &str = "chatos.local_connector.sandbox_id";
const LOCAL_SANDBOX_AGENT_SOCKET_DIR: &str = "/run/chatos-agent";
const LOCAL_SANDBOX_AGENT_SOCKET_PATH: &str = "/run/chatos-agent/agent.sock";

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
    let isolated_network = network_mode.eq_ignore_ascii_case("none");
    let runtime_network = network_mode.to_string();
    let workspace_mount_mode = workspace_mount_mode(permission_profile);
    let tmpfs_size_mb = (resource_limits.disk_mb / 16).clamp(16, 512);
    let home_tmpfs_size_mb = tmpfs_size_mb;
    let workspace_limit_mb = resource_limits
        .disk_mb
        .saturating_sub(tmpfs_size_mb.saturating_add(home_tmpfs_size_mb))
        .max(1);
    let disk_limit_bytes = workspace_limit_mb.saturating_mul(1024 * 1024);
    let sandbox_user = sandbox_user_for_workspace(run_workspace);
    let ipc_volume = if isolated_network {
        let volume = create_local_sandbox_ipc_volume(sandbox_id).await?;
        if let Err(err) = initialize_local_sandbox_ipc_volume(
            sandbox_id,
            image_ref,
            volume.as_str(),
            &sandbox_user,
        )
        .await
        {
            let _ = remove_local_sandbox_ipc_volume(sandbox_id).await;
            return Err(err);
        }
        Some(volume)
    } else {
        None
    };
    let mut command = docker_command();
    command
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(name.as_str())
        .arg("--hostname")
        .arg(name.as_str())
        .arg("--label")
        .arg(format!("{LOCAL_SANDBOX_LABEL}={sandbox_id}"))
        .arg("--network")
        .arg(runtime_network.as_str())
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
    if let Some(volume) = ipc_volume.as_deref() {
        command
            .arg("-e")
            .arg(format!(
                "CHATOS_AGENT_UNIX_SOCKET={LOCAL_SANDBOX_AGENT_SOCKET_PATH}"
            ))
            .arg("-v")
            .arg(format!("{volume}:{LOCAL_SANDBOX_AGENT_SOCKET_DIR}:rw"));
    }
    if !isolated_network {
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
    let output = match command.output().await {
        Ok(output) => output,
        Err(err) => {
            if isolated_network {
                let _ = remove_local_sandbox_ipc_volume(sandbox_id).await;
            }
            return Err(err).context("start local docker sandbox");
        }
    };
    if !output.status.success() {
        if isolated_network {
            let _ = remove_local_sandbox_ipc_volume(sandbox_id).await;
        }
        return Err(anyhow!(
            "docker run failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ));
    }
    let backend_id = String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string();
    if isolated_network {
        if let Err(err) = start_local_sandbox_agent_relay(
            sandbox_id,
            image_ref,
            ipc_volume.as_deref().unwrap_or_default(),
            &sandbox_user,
        )
        .await
        {
            let _ = remove_named_container(local_sandbox_container_name(sandbox_id).as_str()).await;
            let _ = remove_local_sandbox_ipc_volume(sandbox_id).await;
            return Err(err);
        }
    }
    Ok(backend_id)
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
    let mut errors = Vec::new();
    for name in [
        local_sandbox_relay_container_name(sandbox_id),
        local_sandbox_container_name(sandbox_id),
        local_sandbox_ipc_init_container_name(sandbox_id),
    ] {
        if let Err(err) = remove_named_container(name.as_str()).await {
            errors.push(err.to_string());
        }
    }
    if let Err(err) = remove_local_sandbox_internal_network(sandbox_id).await {
        errors.push(err.to_string());
    }
    if let Err(err) = remove_local_sandbox_ipc_volume(sandbox_id).await {
        errors.push(err.to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(errors.join("; ")))
    }
}

pub(crate) async fn destroy_all_local_sandbox_containers() -> Result<()> {
    let containers = docker_command()
        .args([
            "ps",
            "-aq",
            "--filter",
            format!("label={LOCAL_SANDBOX_LABEL}").as_str(),
        ])
        .output()
        .await
        .context("list managed local sandbox containers")?;
    if !containers.status.success() {
        return Err(anyhow!(
            "list managed local sandbox containers failed: {}",
            String::from_utf8_lossy(containers.stderr.as_slice())
        ));
    }
    let container_ids = String::from_utf8_lossy(containers.stdout.as_slice())
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !container_ids.is_empty() {
        let output = docker_command()
            .args(["rm", "-f"])
            .args(container_ids)
            .output()
            .await
            .context("remove managed local sandbox containers")?;
        if !output.status.success() {
            return Err(anyhow!(
                "remove managed local sandbox containers failed: {}",
                String::from_utf8_lossy(output.stderr.as_slice())
            ));
        }
    }

    let networks = docker_command()
        .args([
            "network",
            "ls",
            "-q",
            "--filter",
            format!("label={LOCAL_SANDBOX_LABEL}").as_str(),
        ])
        .output()
        .await
        .context("list managed local sandbox networks")?;
    if !networks.status.success() {
        return Err(anyhow!(
            "list managed local sandbox networks failed: {}",
            String::from_utf8_lossy(networks.stderr.as_slice())
        ));
    }
    let network_ids = String::from_utf8_lossy(networks.stdout.as_slice())
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !network_ids.is_empty() {
        let output = docker_command()
            .args(["network", "rm"])
            .args(network_ids)
            .output()
            .await
            .context("remove managed local sandbox networks")?;
        if !output.status.success() {
            return Err(anyhow!(
                "remove managed local sandbox networks failed: {}",
                String::from_utf8_lossy(output.stderr.as_slice())
            ));
        }
    }
    let volumes = docker_command()
        .args([
            "volume",
            "ls",
            "-q",
            "--filter",
            format!("label={LOCAL_SANDBOX_LABEL}").as_str(),
        ])
        .output()
        .await
        .context("list managed local sandbox volumes")?;
    if !volumes.status.success() {
        return Err(anyhow!(
            "list managed local sandbox volumes failed: {}",
            String::from_utf8_lossy(volumes.stderr.as_slice())
        ));
    }
    let volume_names = String::from_utf8_lossy(volumes.stdout.as_slice())
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !volume_names.is_empty() {
        let output = docker_command()
            .args(["volume", "rm", "-f"])
            .args(volume_names)
            .output()
            .await
            .context("remove managed local sandbox volumes")?;
        if !output.status.success() {
            return Err(anyhow!(
                "remove managed local sandbox volumes failed: {}",
                String::from_utf8_lossy(output.stderr.as_slice())
            ));
        }
    }
    Ok(())
}

async fn create_local_sandbox_ipc_volume(sandbox_id: &str) -> Result<String> {
    let name = local_sandbox_ipc_volume_name(sandbox_id);
    let output = docker_command()
        .args(["volume", "create"])
        .arg("--label")
        .arg(format!("{LOCAL_SANDBOX_LABEL}={sandbox_id}"))
        .arg(name.as_str())
        .output()
        .await
        .context("create isolated local sandbox IPC volume")?;
    if !output.status.success() {
        return Err(anyhow!(
            "create isolated local sandbox IPC volume failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ));
    }
    Ok(name)
}

async fn initialize_local_sandbox_ipc_volume(
    sandbox_id: &str,
    image_ref: &str,
    volume: &str,
    sandbox_user: &SandboxUser,
) -> Result<()> {
    let init_name = local_sandbox_ipc_init_container_name(sandbox_id);
    let command = format!(
        "chown {}:{} {LOCAL_SANDBOX_AGENT_SOCKET_DIR} && chmod 700 {LOCAL_SANDBOX_AGENT_SOCKET_DIR}",
        sandbox_user.uid, sandbox_user.gid
    );
    let output = docker_command()
        .arg("run")
        .arg("--rm")
        .arg("--name")
        .arg(init_name.as_str())
        .arg("--label")
        .arg(format!("{LOCAL_SANDBOX_LABEL}={sandbox_id}"))
        .arg("--network")
        .arg("none")
        .arg("--read-only")
        .arg("--cap-drop")
        .arg("ALL")
        .arg("--cap-add")
        .arg("CHOWN")
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("--user")
        .arg("0:0")
        .arg("--entrypoint")
        .arg("/bin/sh")
        .arg("-v")
        .arg(format!("{volume}:{LOCAL_SANDBOX_AGENT_SOCKET_DIR}:rw"))
        .arg(image_ref)
        .arg("-c")
        .arg(command)
        .output()
        .await
        .context("initialize isolated local sandbox IPC volume")?;
    if !output.status.success() {
        return Err(anyhow!(
            "initialize isolated local sandbox IPC volume failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ));
    }
    Ok(())
}

async fn start_local_sandbox_agent_relay(
    sandbox_id: &str,
    image_ref: &str,
    ipc_volume: &str,
    sandbox_user: &SandboxUser,
) -> Result<()> {
    let relay_name = local_sandbox_relay_container_name(sandbox_id);
    let output = docker_command()
        .arg("create")
        .arg("--name")
        .arg(relay_name.as_str())
        .arg("--hostname")
        .arg(relay_name.as_str())
        .arg("--label")
        .arg(format!("{LOCAL_SANDBOX_LABEL}={sandbox_id}"))
        .arg("--network")
        .arg("bridge")
        .arg("--cpus")
        .arg("0.25")
        .arg("--memory")
        .arg("64m")
        .arg("--pids-limit")
        .arg("32")
        .arg("-p")
        .arg(format!("127.0.0.1::{DEFAULT_LOCAL_SANDBOX_AGENT_PORT}"))
        .arg("--read-only")
        .arg("--cap-drop")
        .arg("ALL")
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("--user")
        .arg(sandbox_user.spec.as_str())
        .arg("--tmpfs")
        .arg("/tmp:rw,nosuid,nodev,size=16m,mode=1777")
        .arg("-v")
        .arg(format!("{ipc_volume}:{LOCAL_SANDBOX_AGENT_SOCKET_DIR}:rw"))
        .arg(image_ref)
        .arg("--internal-agent-relay")
        .arg(LOCAL_SANDBOX_AGENT_SOCKET_PATH)
        .output()
        .await
        .context("create local sandbox agent relay")?;
    if !output.status.success() {
        return Err(anyhow!(
            "create local sandbox agent relay failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        ));
    }

    let start = match docker_command()
        .arg("start")
        .arg(relay_name.as_str())
        .output()
        .await
    {
        Ok(output) => output,
        Err(err) => {
            let _ = remove_named_container(relay_name.as_str()).await;
            return Err(err).context("start local sandbox agent relay");
        }
    };
    if !start.status.success() {
        let _ = remove_named_container(relay_name.as_str()).await;
        return Err(anyhow!(
            "start local sandbox agent relay failed: {}",
            String::from_utf8_lossy(start.stderr.as_slice())
        ));
    }
    Ok(())
}

async fn remove_named_container(name: &str) -> Result<()> {
    let output = docker_command()
        .arg("rm")
        .arg("-f")
        .arg(name)
        .output()
        .await
        .with_context(|| format!("remove managed local sandbox container {name}"))?;
    let stderr = String::from_utf8_lossy(output.stderr.as_slice());
    if output.status.success() || stderr.contains("No such container") {
        Ok(())
    } else {
        Err(anyhow!(
            "remove managed local sandbox container {name} failed: {stderr}"
        ))
    }
}

async fn remove_local_sandbox_internal_network(sandbox_id: &str) -> Result<()> {
    let output = docker_command()
        .args(["network", "rm"])
        .arg(local_sandbox_network_name(sandbox_id))
        .output()
        .await
        .context("remove isolated local sandbox network")?;
    let stderr = String::from_utf8_lossy(output.stderr.as_slice());
    if output.status.success() || stderr.contains("No such network") || stderr.contains("not found")
    {
        Ok(())
    } else {
        Err(anyhow!(
            "remove isolated local sandbox network failed: {stderr}"
        ))
    }
}

async fn remove_local_sandbox_ipc_volume(sandbox_id: &str) -> Result<()> {
    let name = local_sandbox_ipc_volume_name(sandbox_id);
    let output = docker_command()
        .args(["volume", "rm", "-f"])
        .arg(name.as_str())
        .output()
        .await
        .context("remove isolated local sandbox IPC volume")?;
    let stderr = String::from_utf8_lossy(output.stderr.as_slice());
    if output.status.success() || stderr.contains("No such volume") || stderr.contains("not found")
    {
        Ok(())
    } else {
        Err(anyhow!(
            "remove isolated local sandbox IPC volume failed: {stderr}"
        ))
    }
}

pub(crate) async fn published_local_sandbox_agent_endpoint(sandbox_id: &str) -> Option<String> {
    for container_name in [
        local_sandbox_relay_container_name(sandbox_id),
        local_sandbox_container_name(sandbox_id),
    ] {
        let output = docker_command()
            .arg("port")
            .arg(container_name)
            .arg(format!("{DEFAULT_LOCAL_SANDBOX_AGENT_PORT}/tcp"))
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(output.stdout.as_slice());
        let line = stdout.lines().next()?.trim();
        let host_port = line.rsplit(':').next()?.trim();
        if !host_port.is_empty() {
            return Some(format!("http://127.0.0.1:{host_port}"));
        }
    }
    None
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

fn local_sandbox_relay_container_name(sandbox_id: &str) -> String {
    format!("chatos-local-relay-{sandbox_id}")
}

fn local_sandbox_ipc_init_container_name(sandbox_id: &str) -> String {
    format!("chatos-local-ipc-init-{sandbox_id}")
}

fn local_sandbox_ipc_volume_name(sandbox_id: &str) -> String {
    format!("chatos-local-ipc-{sandbox_id}")
}

fn local_sandbox_network_name(sandbox_id: &str) -> String {
    format!("chatos-local-net-{sandbox_id}")
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
