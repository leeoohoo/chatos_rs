// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
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
        package_file_sha256: &BTreeMap<String, String>,
    ) -> Result<(McpStdioServer, Arc<PluginStdioSandboxRuntime>)> {
        self.prepare_inner(
            plugin_storage_root,
            plugin_root,
            server,
            environment_names,
            package_file_sha256,
            None,
        )
    }

    pub(super) fn prepare_with_workspace_write(
        &self,
        plugin_storage_root: &Path,
        plugin_root: &Path,
        server: &McpStdioServer,
        environment_names: impl IntoIterator<Item = String>,
        package_file_sha256: &BTreeMap<String, String>,
        workspace_root: &Path,
    ) -> Result<(McpStdioServer, Arc<PluginStdioSandboxRuntime>)> {
        self.prepare_inner(
            plugin_storage_root,
            plugin_root,
            server,
            environment_names,
            package_file_sha256,
            Some(workspace_root),
        )
    }

    fn prepare_inner(
        &self,
        plugin_storage_root: &Path,
        plugin_root: &Path,
        server: &McpStdioServer,
        environment_names: impl IntoIterator<Item = String>,
        package_file_sha256: &BTreeMap<String, String>,
        workspace_root: Option<&Path>,
    ) -> Result<(McpStdioServer, Arc<PluginStdioSandboxRuntime>)> {
        let runtime = Arc::new(PluginStdioSandboxRuntime::create(
            plugin_storage_root,
            package_file_sha256,
        )?);
        let target_cwd = server.cwd.as_deref().map(Path::new).unwrap_or(plugin_root);
        let mut args = vec![
            PLUGIN_STDIO_WRAPPER_MODE.to_string(),
            "--plugin-root".to_string(),
            plugin_root.to_string_lossy().into_owned(),
            "--sandbox-id".to_string(),
            runtime.sandbox_id.clone(),
            "--state-root".to_string(),
            runtime.state_root.to_string_lossy().into_owned(),
            "--cache-root".to_string(),
            runtime.cache_root.to_string_lossy().into_owned(),
            "--temp-root".to_string(),
            runtime.temp_root.to_string_lossy().into_owned(),
            "--package-index".to_string(),
            runtime.package_index.to_string_lossy().into_owned(),
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
    sandbox_id: String,
    root: PathBuf,
    state_root: PathBuf,
    cache_root: PathBuf,
    temp_root: PathBuf,
    package_index: PathBuf,
}

impl PluginStdioSandboxRuntime {
    fn create(
        plugin_storage_root: &Path,
        package_file_sha256: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let parent = plugin_storage_root.join("runtime").join("stdio");
        create_private_directory_all(parent.as_path())?;
        let sandbox_id = Uuid::new_v4().to_string();
        let root = parent.join(sandbox_id.as_str());
        create_private_directory(root.as_path())?;
        match Self::create_inner(sandbox_id, root.as_path(), package_file_sha256) {
            Ok(runtime) => Ok(runtime),
            Err(error) => {
                let _ = fs::remove_dir_all(root.as_path());
                Err(error)
            }
        }
    }

    fn create_inner(
        sandbox_id: String,
        root: &Path,
        package_file_sha256: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let state_root = root.join("state");
        let cache_root = root.join("cache");
        let temp_root = root.join("tmp");
        for directory in [&state_root, &cache_root, &temp_root] {
            create_private_directory(directory.as_path())?;
        }
        let package_index = root.join("package-index.json");
        let package_index_bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "files": package_file_sha256,
        }))
        .context("serialize Plugin stdio signed package index")?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(package_index.as_path())
            .with_context(|| {
                format!(
                    "create Plugin stdio signed package index {}",
                    package_index.display()
                )
            })?;
        file.write_all(package_index_bytes.as_slice())
            .context("write Plugin stdio signed package index")?;
        file.sync_all()
            .context("sync Plugin stdio signed package index")?;
        set_private_file_permissions(package_index.as_path())?;
        Ok(Self {
            sandbox_id,
            root: root.to_path_buf(),
            state_root,
            cache_root,
            temp_root,
            package_index,
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
        #[cfg(windows)]
        cleanup_windows_appcontainer(self.sandbox_id.as_str());
        let _ = fs::remove_dir_all(self.root.as_path());
    }
}

#[cfg(windows)]
fn cleanup_windows_appcontainer(sandbox_id: &str) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Isolation::DeleteAppContainerProfile;

    let name = format!("chatos.plugin.{}", sandbox_id.replace('-', ""));
    let wide = std::ffi::OsStr::new(name.as_str())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let _ = DeleteAppContainerProfile(wide.as_ptr());
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

fn set_private_permissions(_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700)).with_context(|| {
            format!(
                "set Plugin stdio runtime directory permissions {}",
                _path.display()
            )
        })?;
    }
    Ok(())
}

fn set_private_file_permissions(_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "set Plugin stdio runtime file permissions {}",
                _path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PluginStdioSandboxLauncher;
    use chatos_mcp_runtime::McpStdioServer;
    use std::collections::BTreeMap;

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
                &BTreeMap::from([("hook".to_string(), "a".repeat(64))]),
                workspace_root.as_path(),
            )
            .expect("prepare writable Hook sandbox");
        let args = wrapped.args.expect("wrapper arguments");
        let sandbox_index = args
            .iter()
            .position(|arg| arg == "--sandbox-id")
            .expect("sandbox ID option");
        assert!(uuid::Uuid::parse_str(
            args.get(sandbox_index + 1)
                .expect("sandbox ID value")
                .as_str()
        )
        .is_ok());
        let package_index = args
            .iter()
            .position(|arg| arg == "--package-index")
            .and_then(|index| args.get(index + 1))
            .expect("signed package index option");
        let package_index: serde_json::Value = serde_json::from_slice(
            std::fs::read(package_index)
                .expect("read signed package index")
                .as_slice(),
        )
        .expect("parse signed package index");
        assert_eq!(
            package_index
                .pointer("/files/hook")
                .and_then(|value| value.as_str()),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
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
