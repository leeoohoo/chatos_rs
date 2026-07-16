// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
pub(super) fn sandbox_workspace_root(workspace_dir: &str) -> Result<PathBuf, String> {
    let root = Path::new(workspace_dir).join(".chatos").join("task-runner");
    fs::create_dir_all(&root).map_err(|err| {
        format!(
            "create sandbox workspace root {} failed: {err}",
            root.display()
        )
    })?;
    Ok(root)
}

pub(super) fn is_local_connector_sandbox_manager(base_url: &str) -> bool {
    base_url.contains("/api/local-connectors/sandbox-facade/")
}

pub(super) fn sandbox_baseline_workspace(run_workspace: &str) -> Result<String, String> {
    let run_workspace = Path::new(run_workspace);
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "invalid sandbox run workspace path".to_string())?;
    Ok(run_root
        .join("baseline")
        .join("workspace")
        .to_string_lossy()
        .to_string())
}

pub(super) fn copy_workspace_to_sandbox(source: &str, destination: &str) -> Result<(), String> {
    super::super::workspace_snapshot::copy_workspace_snapshot(source, destination)?;
    prepare_sandbox_workspace_owner(Path::new(destination))
}

#[cfg(unix)]
fn prepare_sandbox_workspace_owner(root: &Path) -> Result<(), String> {
    use std::os::unix::fs::{chown, PermissionsExt};

    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(path.as_path())
            .map_err(|err| format!("inspect sandbox workspace {} failed: {err}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(path.as_path())
                .map_err(|err| format!("read sandbox workspace {} failed: {err}", path.display()))?
            {
                pending.push(
                    entry
                        .map_err(|err| format!("read sandbox workspace entry failed: {err}"))?
                        .path(),
                );
            }
        }
        if let Err(err) = chown(path.as_path(), Some(1000), Some(1000)) {
            if err.kind() != std::io::ErrorKind::PermissionDenied {
                return Err(format!(
                    "set sandbox workspace owner for {} failed: {err}",
                    path.display()
                ));
            }
            let mut permissions = metadata.permissions();
            let fallback_mode = if metadata.is_dir() {
                0o777
            } else {
                (permissions.mode() & 0o111) | 0o666
            };
            permissions.set_mode(fallback_mode);
            fs::set_permissions(path.as_path(), permissions).map_err(|permissions_err| {
                format!(
                    "make sandbox workspace {} accessible after chown failed ({err}): {permissions_err}",
                    path.display()
                )
            })?;
        } else {
            let mut permissions = metadata.permissions();
            let safe_mode = if metadata.is_dir() {
                (permissions.mode() & 0o555) | 0o700
            } else {
                (permissions.mode() & 0o555) | 0o600
            };
            permissions.set_mode(safe_mode);
            fs::set_permissions(path.as_path(), permissions).map_err(|err| {
                format!(
                    "set sandbox workspace permissions for {} failed: {err}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn prepare_sandbox_workspace_owner(_root: &Path) -> Result<(), String> {
    Ok(())
}
