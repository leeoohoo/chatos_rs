// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use chatos_mcp::TerminalCommandPermissions;
use chatos_sandbox_contract::{
    FileSystemAccessMode, FileSystemPath, FileSystemPermissionPolicy, FileSystemSandboxEntry,
    FileSystemSpecialPath, GrantedPermissionProfile, NetworkPermissionPolicy, NetworkRequirements,
    PermissionProfileId,
};
use globset::GlobSet;
use globset::{GlobBuilder, GlobSetBuilder};
use tokio::process::{Child, Command};
use walkdir::WalkDir;

use crate::config::ServerConfig;
use crate::network_proxy::{NetworkProxyEndpoints, NetworkProxyRuntime};

mod config;
mod permissions;
mod platform;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;

use self::config::CommandSandboxBackend;
pub(crate) use self::config::{CommandSandboxConfig, FileToolAccessPolicy};
use self::permissions::TransientPath;
#[cfg(target_os = "linux")]
use self::platform::prepare_linux_command;
#[cfg(target_os = "macos")]
use self::platform::prepare_macos_command;
use self::platform::{
    command_network_access, direct_shell_command, validate_permission_context, CommandNetworkAccess,
};

pub(crate) struct PreparedSandboxCommand {
    command: Command,
    cleanup: Vec<TransientPath>,
}

pub(crate) struct SpawnedSandboxCommand {
    pub(crate) child: Child,
    pub(crate) cleanup: CommandSandboxCleanup,
}

pub(crate) struct CommandSandboxCleanup {
    paths: Vec<TransientPath>,
}

impl CommandSandboxCleanup {
    pub(crate) fn run(self) {
        for path in self.paths.into_iter().rev() {
            path.remove_if_unchanged();
        }
    }
}

impl PreparedSandboxCommand {
    pub(crate) fn new(
        config: &CommandSandboxConfig,
        shell: &str,
        command: &str,
        cwd: &Path,
        permissions: &TerminalCommandPermissions,
    ) -> Result<Self, String> {
        let granted = validate_permission_context(permissions)?;
        if config.backend == CommandSandboxBackend::External {
            if granted.is_some() || permissions.requested.is_some() {
                return Err(
                    "temporary permission overlays are unavailable in an externally sandboxed runtime"
                        .to_string(),
                );
            }
            return Ok(Self {
                command: direct_shell_command(shell, command),
                cleanup: Vec::new(),
            });
        }

        let network_access = command_network_access(config, granted);
        if config.permission_profile == PermissionProfileId::FullAccess
            && matches!(network_access, CommandNetworkAccess::Full)
        {
            return Ok(Self {
                command: direct_shell_command(shell, command),
                cleanup: Vec::new(),
            });
        }

        #[cfg(target_os = "macos")]
        {
            prepare_macos_command(config, shell, command, cwd, granted, network_access)
        }
        #[cfg(target_os = "linux")]
        {
            prepare_linux_command(config, shell, command, cwd, granted, network_access)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = (shell, command, cwd, granted, network_access);
            Err("native command sandbox is unsupported on this operating system".to_string())
        }
    }

    pub(crate) fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    pub(crate) fn spawn(mut self) -> Result<SpawnedSandboxCommand, String> {
        match self.command.spawn() {
            Ok(child) => Ok(SpawnedSandboxCommand {
                child,
                cleanup: CommandSandboxCleanup {
                    paths: self.cleanup,
                },
            }),
            Err(err) => {
                for path in self.cleanup.drain(..).rev() {
                    path.remove_if_unchanged();
                }
                Err(err.to_string())
            }
        }
    }
}
