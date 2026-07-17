// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::config::*;
use super::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub(super) use linux::prepare_linux_command;
#[cfg(target_os = "macos")]
pub(super) use macos::prepare_macos_command;

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
#[derive(Debug, Clone)]
pub(super) enum CommandNetworkAccess {
    Disabled,
    Proxy(NetworkProxyEndpoints),
    Full,
}

pub(super) fn command_network_access(
    config: &CommandSandboxConfig,
    granted: Option<&GrantedPermissionProfile>,
) -> CommandNetworkAccess {
    if granted
        .and_then(|grant| grant.network.as_ref())
        .and_then(|network| network.enabled)
        == Some(true)
        || config.network_unrestricted
    {
        return CommandNetworkAccess::Full;
    }
    config
        .network_proxy
        .as_ref()
        .map(|proxy| CommandNetworkAccess::Proxy(proxy.endpoints().clone()))
        .unwrap_or(CommandNetworkAccess::Disabled)
}

pub(super) fn direct_shell_command(shell: &str, command: &str) -> Command {
    let mut process = Command::new(shell);
    process.arg("-lc").arg(command);
    process
}

pub(super) fn validate_permission_context(
    permissions: &TerminalCommandPermissions,
) -> Result<Option<&GrantedPermissionProfile>, String> {
    match (&permissions.requested, &permissions.granted) {
        (None, None) => Ok(None),
        (Some(_), None) => Err("requested permission overlay was not granted".to_string()),
        (None, Some(_)) => Err("granted permission overlay has no matching request".to_string()),
        (Some(requested), Some(granted)) => {
            requested.validate()?;
            if let Some(file_system) = &granted.file_system {
                file_system.validate()?;
            }
            if !requested.allows_grant(granted) {
                return Err("granted permission overlay exceeds the request".to_string());
            }
            Ok(Some(granted))
        }
    }
}
