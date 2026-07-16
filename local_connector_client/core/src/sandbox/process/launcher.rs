// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use crate::sandbox::types::LocalSandboxResourceLimits;
use anyhow::Result;

pub(super) struct NativeLauncherSpec<'a> {
    pub(super) agent: &'a Path,
    pub(super) workspace: &'a Path,
    pub(super) home: &'a Path,
    pub(super) temp: &'a Path,
    pub(super) resource_limits: &'a LocalSandboxResourceLimits,
    pub(super) environment: BTreeMap<String, String>,
}

pub(super) fn native_sandbox_command(
    spec: NativeLauncherSpec<'_>,
) -> Result<tokio::process::Command> {
    // The agent is trusted broker code. Each untrusted shell command receives its own native
    // Seatbelt/Bubblewrap policy inside `chatos_sandbox_mcp_server`, which is what makes precise
    // command-scoped permission overlays possible.
    let mut command = tokio::process::Command::new(spec.agent);
    command
        .current_dir(spec.workspace)
        .env_clear()
        .envs(safe_base_environment(spec.home, spec.temp))
        .envs(spec.environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group_and_limits(&mut command, spec.resource_limits)?;
    Ok(command)
}

fn safe_base_environment(home: &Path, temp: &Path) -> BTreeMap<String, String> {
    let mut environment = BTreeMap::new();
    for name in ["LANG", "LC_ALL", "LOGNAME", "PATH", "SHELL", "TERM", "USER"] {
        if let Ok(value) = std::env::var(name) {
            if !value.trim().is_empty() {
                environment.insert(name.to_string(), value);
            }
        }
    }
    environment.insert("HOME".to_string(), home.to_string_lossy().to_string());
    environment.insert("TMPDIR".to_string(), temp.to_string_lossy().to_string());
    environment.insert(
        "XDG_CACHE_HOME".to_string(),
        home.join(".cache").to_string_lossy().to_string(),
    );
    environment.insert(
        "XDG_CONFIG_HOME".to_string(),
        home.join(".config").to_string_lossy().to_string(),
    );
    environment.insert(
        "XDG_STATE_HOME".to_string(),
        home.join(".local/state").to_string_lossy().to_string(),
    );
    environment
}

#[cfg(unix)]
fn configure_process_group_and_limits(
    command: &mut tokio::process::Command,
    _limits: &LocalSandboxResourceLimits,
) -> Result<()> {
    use std::os::unix::process::CommandExt;

    command.as_std_mut().process_group(0);
    Ok(())
}

#[cfg(not(unix))]
fn configure_process_group_and_limits(
    _command: &mut tokio::process::Command,
    _limits: &LocalSandboxResourceLimits,
) -> Result<()> {
    Ok(())
}
