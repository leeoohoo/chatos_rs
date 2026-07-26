// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use chatos_mcp_runtime::McpStdioServer;
use uuid::Uuid;

use crate::sandbox::process::plugin_stdio_sandbox_agent_executable;

const PLUGIN_STDIO_WRAPPER_MODE: &str = "--internal-plugin-stdio-wrapper";

#[derive(Debug, Clone)]
pub(super) struct PluginStdioSandboxLauncher {
    agent: PathBuf,
}

impl PluginStdioSandboxLauncher {
    pub(super) fn discover() -> Result<Self> {
        Ok(Self {
            agent: plugin_stdio_sandbox_agent_executable().map_err(anyhow::Error::msg)?,
        })
    }

    pub(super) fn prepare(
        &self,
        plugin_storage_root: &Path,
        plugin_root: &Path,
        server: &McpStdioServer,
        environment_names: impl IntoIterator<Item = String>,
    ) -> Result<(McpStdioServer, Arc<PluginStdioSandboxRuntime>)> {
        self.prepare_inner(
            plugin_storage_root,
            plugin_root,
            server,
            environment_names,
            None,
        )
    }

    pub(super) fn prepare_with_workspace_write(
        &self,
        plugin_storage_root: &Path,
        plugin_root: &Path,
        server: &McpStdioServer,
        environment_names: impl IntoIterator<Item = String>,
        workspace_root: &Path,
    ) -> Result<(McpStdioServer, Arc<PluginStdioSandboxRuntime>)> {
        self.prepare_inner(
            plugin_storage_root,
            plugin_root,
            server,
            environment_names,
            Some(workspace_root),
        )
    }

    fn prepare_inner(
        &self,
        plugin_storage_root: &Path,
        plugin_root: &Path,
        server: &McpStdioServer,
        environment_names: impl IntoIterator<Item = String>,
        workspace_root: Option<&Path>,
    ) -> Result<(McpStdioServer, Arc<PluginStdioSandboxRuntime>)> {
        let runtime = Arc::new(PluginStdioSandboxRuntime::create(plugin_storage_root)?);
        let target_cwd = server.cwd.as_deref().map(Path::new).unwrap_or(plugin_root);
        let mut args = vec![
            PLUGIN_STDIO_WRAPPER_MODE.to_string(),
            "--plugin-root".to_string(),
            plugin_root.to_string_lossy().into_owned(),
            "--state-root".to_string(),
            runtime.state_root.to_string_lossy().into_owned(),
            "--cache-root".to_string(),
            runtime.cache_root.to_string_lossy().into_owned(),
            "--temp-root".to_string(),
            runtime.temp_root.to_string_lossy().into_owned(),
            "--cwd".to_string(),
            target_cwd.to_string_lossy().into_owned(),
        ];
        if let Some(workspace_root) = workspace_root {
            args.push("--workspace-root".to_string());
            args.push(workspace_root.to_string_lossy().into_owned());
        }
        for name in environment_names {
            args.push("--env".to_string());
            args.push(name);
        }
        args.push("--".to_string());
        args.push(server.command.clone());
        args.extend(server.args.clone().unwrap_or_default());
        let mut wrapped = McpStdioServer::new(server.name.clone(), self.agent.to_string_lossy())
            .with_args(args)
            .with_cwd(runtime.root.to_string_lossy());
        if let Some(user_id) = &server.user_id {
            wrapped = wrapped.with_user_id(user_id.clone());
        }
        Ok((wrapped, runtime))
    }
}

pub(super) struct PluginStdioSandboxRuntime {
    root: PathBuf,
    state_root: PathBuf,
    cache_root: PathBuf,
    temp_root: PathBuf,
}

impl PluginStdioSandboxRuntime {
    fn create(plugin_storage_root: &Path) -> Result<Self> {
        let parent = plugin_storage_root.join("runtime").join("stdio");
        create_private_directory_all(parent.as_path())?;
        let root = parent.join(Uuid::new_v4().to_string());
        create_private_directory(root.as_path())?;
        let state_root = root.join("state");
        let cache_root = root.join("cache");
        let temp_root = root.join("tmp");
        for directory in [&state_root, &cache_root, &temp_root] {
            if let Err(error) = create_private_directory(directory.as_path()) {
                let _ = fs::remove_dir_all(root.as_path());
                return Err(error);
            }
        }
        Ok(Self {
            root,
            state_root,
            cache_root,
            temp_root,
        })
    }
}

impl fmt::Debug for PluginStdioSandboxRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginStdioSandboxRuntime")
            .field("isolated", &true)
            .finish_non_exhaustive()
    }
}

impl Drop for PluginStdioSandboxRuntime {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.root.as_path());
    }
}

fn create_private_directory_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("create Plugin stdio runtime directory {}", path.display()))?;
    set_private_permissions(path)
}

fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir(path)
        .with_context(|| format!("create Plugin stdio runtime directory {}", path.display()))?;
    set_private_permissions(path)
}

fn set_private_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).with_context(|| {
            format!(
                "set Plugin stdio runtime directory permissions {}",
                path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PluginStdioSandboxLauncher;
    use chatos_mcp_runtime::McpStdioServer;

    #[test]
    fn writable_hook_wrapper_receives_only_the_explicit_workspace_root() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let plugin_storage = temp.path().join("plugins");
        let plugin_root = temp.path().join("installed-plugin");
        let workspace_root = temp.path().join("workspace");
        std::fs::create_dir_all(plugin_storage.as_path()).expect("plugin storage");
        std::fs::create_dir_all(plugin_root.as_path()).expect("Plugin root");
        std::fs::create_dir_all(workspace_root.as_path()).expect("workspace root");
        let launcher = PluginStdioSandboxLauncher {
            agent: temp.path().join("sandbox-agent"),
        };
        let server = McpStdioServer::new("hook", plugin_root.join("hook").to_string_lossy())
            .with_cwd(plugin_root.to_string_lossy());

        let (wrapped, _runtime) = launcher
            .prepare_with_workspace_write(
                plugin_storage.as_path(),
                plugin_root.as_path(),
                &server,
                Vec::<String>::new(),
                workspace_root.as_path(),
            )
            .expect("prepare writable Hook sandbox");
        let args = wrapped.args.expect("wrapper arguments");
        let workspace_index = args
            .iter()
            .position(|arg| arg == "--workspace-root")
            .expect("workspace option");
        assert_eq!(
            args.get(workspace_index + 1).map(String::as_str),
            Some(workspace_root.to_string_lossy().as_ref())
        );
        assert_eq!(
            args.iter().filter(|arg| *arg == "--workspace-root").count(),
            1
        );
    }
}
