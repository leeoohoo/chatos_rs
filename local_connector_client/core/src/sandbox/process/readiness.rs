// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Stdio;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::time::Duration;

use chatos_sandbox_contract::{
    SandboxBackendCapability, SandboxBackendKind, SandboxBackendReadinessStatus,
};

pub(crate) async fn native_process_sandbox_capability() -> SandboxBackendCapability {
    let agent = match native_sandbox_agent_executable() {
        Ok(agent) => agent,
        Err(message) => {
            return capability(SandboxBackendReadinessStatus::SetupRequired, false, message)
        }
    };

    #[cfg(target_os = "macos")]
    {
        return macos_capability(agent.as_path()).await;
    }
    #[cfg(target_os = "linux")]
    {
        return linux_capability(agent.as_path()).await;
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = agent;
        capability(
            SandboxBackendReadinessStatus::Unsupported,
            false,
            "Native process sandbox is currently supported on macOS and Linux only".to_string(),
        )
    }
}

pub(crate) fn native_sandbox_agent_executable() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("LOCAL_CONNECTOR_SANDBOX_PROCESS_AGENT") {
        return validate_agent_candidate(PathBuf::from(path)).ok_or_else(|| {
            "LOCAL_CONNECTOR_SANDBOX_PROCESS_AGENT does not point to an executable sandbox agent"
                .to_string()
        });
    }

    let mut directories = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            directories.push(parent.to_path_buf());
            if parent.file_name().and_then(|value| value.to_str()) == Some("deps") {
                if let Some(debug_or_release) = parent.parent() {
                    directories.push(debug_or_release.to_path_buf());
                }
            }
        }
    }
    for directory in directories {
        for name in ["chatos_sandbox_mcp_server", "chatos-sandbox-mcp-server"] {
            if let Some(path) = validate_agent_candidate(directory.join(name)) {
                return Ok(path);
            }
        }
    }
    Err(
        "Native sandbox agent is not installed beside Local Connector; rebuild or reinstall the client"
            .to_string(),
    )
}

pub(crate) fn plugin_stdio_sandbox_agent_executable() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let agent = native_sandbox_agent_executable()?;
        let sandbox_exec = Path::new("/usr/bin/sandbox-exec");
        if !executable_file(sandbox_exec) {
            return Err("macOS Seatbelt launcher /usr/bin/sandbox-exec is unavailable".to_string());
        }
        Ok(agent)
    }
    #[cfg(target_os = "linux")]
    {
        let agent = native_sandbox_agent_executable()?;
        system_bwrap_executable().ok_or_else(|| {
            "Linux Bubblewrap launcher is unavailable or is not a trusted system executable"
                .to_string()
        })?;
        Ok(agent)
    }
    #[cfg(windows)]
    {
        // The wrapper creates an ephemeral zero-capability AppContainer and fails closed if the
        // profile, signed-package staging ACL, or Job Object cannot be established.
        native_sandbox_agent_executable()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Err(
            "Plugin stdio OS sandbox is currently supported only on macOS, Linux, and Windows"
                .to_string(),
        )
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn system_bwrap_executable() -> Option<PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut candidates = vec![PathBuf::from("/usr/bin/bwrap"), PathBuf::from("/bin/bwrap")];
    if let Some(search_path) = std::env::var_os("PATH") {
        candidates
            .extend(std::env::split_paths(&search_path).map(|directory| directory.join("bwrap")));
    }
    let mut visited = std::collections::BTreeSet::new();
    for candidate in candidates {
        let Ok(candidate) = candidate.canonicalize() else {
            continue;
        };
        if !visited.insert(candidate.clone()) {
            continue;
        }
        let Ok(metadata) = candidate.metadata() else {
            continue;
        };
        if executable_file(candidate.as_path())
            && metadata.uid() == 0
            && metadata.permissions().mode() & 0o022 == 0
        {
            return Some(candidate);
        }
    }
    None
}

fn validate_agent_candidate(path: PathBuf) -> Option<PathBuf> {
    let path = path.canonicalize().ok()?;
    executable_file(path.as_path()).then_some(path)
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
async fn macos_capability(agent: &Path) -> SandboxBackendCapability {
    const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
    if !executable_file(Path::new(SANDBOX_EXEC)) {
        return capability(
            SandboxBackendReadinessStatus::Unsupported,
            false,
            "macOS Seatbelt launcher /usr/bin/sandbox-exec is unavailable".to_string(),
        );
    }
    let profile = format!(
        "{}\n(allow file-read*)\n",
        include_str!("seatbelt_base_policy.sbpl")
    );
    let probe = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new(SANDBOX_EXEC)
            .arg("-p")
            .arg(profile)
            .arg("--")
            .arg("/usr/bin/true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;
    match probe {
        Ok(Ok(output)) if output.status.success() => capability(
            SandboxBackendReadinessStatus::Ready,
            true,
            format!(
                "macOS Seatbelt is ready; sandbox agent: {}; disk quota and process-tree cleanup are enforced, while native CPU/memory/process-count hard limits are not yet available",
                agent.display()
            ),
        ),
        Ok(Ok(output)) => capability(
            SandboxBackendReadinessStatus::SetupRequired,
            false,
            format!(
                "macOS Seatbelt probe failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ),
        Ok(Err(err)) => capability(
            SandboxBackendReadinessStatus::SetupRequired,
            false,
            format!("macOS Seatbelt probe could not start: {err}"),
        ),
        Err(_) => capability(
            SandboxBackendReadinessStatus::SetupRequired,
            false,
            "macOS Seatbelt readiness probe timed out".to_string(),
        ),
    }
}

#[cfg(target_os = "linux")]
async fn linux_capability(agent: &Path) -> SandboxBackendCapability {
    let Some(bwrap) = system_bwrap_executable() else {
        return capability(
            SandboxBackendReadinessStatus::SetupRequired,
            false,
            "Bubblewrap is not available on PATH; install the distribution bwrap package"
                .to_string(),
        );
    };
    let probe = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new(&bwrap)
            .args([
                "--unshare-user",
                "--unshare-net",
                "--ro-bind",
                "/",
                "/",
                "/bin/true",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;
    match probe {
        Ok(Ok(output)) if output.status.success() => capability(
            SandboxBackendReadinessStatus::Ready,
            true,
            format!(
                "Linux Bubblewrap is ready; launcher: {}; sandbox agent: {}; disk quota and process-tree cleanup are enforced, while native CPU/memory/process-count hard limits are not yet available",
                bwrap.display(),
                agent.display()
            ),
        ),
        Ok(Ok(output)) => capability(
            SandboxBackendReadinessStatus::SetupRequired,
            false,
            format!(
                "Bubblewrap cannot create the required user/network namespaces: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ),
        Ok(Err(err)) => capability(
            SandboxBackendReadinessStatus::SetupRequired,
            false,
            format!("Bubblewrap readiness probe could not start: {err}"),
        ),
        Err(_) => capability(
            SandboxBackendReadinessStatus::SetupRequired,
            false,
            "Bubblewrap readiness probe timed out".to_string(),
        ),
    }
}

fn capability(
    status: SandboxBackendReadinessStatus,
    isolation_ready: bool,
    message: String,
) -> SandboxBackendCapability {
    SandboxBackendCapability {
        backend: SandboxBackendKind::LocalProcess,
        status,
        selectable: status == SandboxBackendReadinessStatus::Ready,
        filesystem_isolation: isolation_ready,
        network_isolation: isolation_ready,
        process_tree_control: isolation_ready,
        message,
    }
}
