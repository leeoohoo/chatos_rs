// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Component, Path, PathBuf};

use super::SandboxGeneratedConfigFile;
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

pub(super) fn write_generated_config_files(
    workspace: &str,
    files: &[SandboxGeneratedConfigFile],
) -> Result<(), String> {
    const MAX_FILES: usize = 100;
    const MAX_FILE_BYTES: usize = 1024 * 1024;
    const MAX_TOTAL_BYTES: usize = 5 * 1024 * 1024;

    if files.len() > MAX_FILES {
        return Err(format!(
            "generated config file count exceeds limit: {} > {MAX_FILES}",
            files.len()
        ));
    }
    let total_bytes = files
        .iter()
        .try_fold(0usize, |total, file| total.checked_add(file.content.len()))
        .ok_or_else(|| "generated config total size overflow".to_string())?;
    if total_bytes > MAX_TOTAL_BYTES {
        return Err(format!(
            "generated config total size exceeds limit: {total_bytes} > {MAX_TOTAL_BYTES}"
        ));
    }

    let root = Path::new(workspace);
    fs::create_dir_all(root)
        .map_err(|err| format!("create generated config workspace failed: {err}"))?;
    for file in files {
        if file.content.len() > MAX_FILE_BYTES {
            return Err(format!(
                "generated config file exceeds size limit: {}",
                file.path
            ));
        }
        let relative = normalize_generated_config_path(file.path.as_str())?;
        let target = root.join(relative.as_path());
        reject_symlink_path(root, relative.parent())?;
        if let Ok(metadata) = fs::symlink_metadata(target.as_path()) {
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "generated config target cannot be a symlink: {}",
                    file.path
                ));
            }
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create generated config directory failed: {err}"))?;
        }
        fs::write(target.as_path(), file.content.as_bytes())
            .map_err(|err| format!("write generated config {} failed: {err}", file.path))?;
    }
    prepare_sandbox_workspace_owner(root)
}

fn normalize_generated_config_path(value: &str) -> Result<PathBuf, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("generated config path is empty".to_string());
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(value).components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err("generated config path cannot contain ..".to_string())
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("generated config path must be relative".to_string())
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err("generated config path is empty".to_string());
    }
    Ok(normalized)
}

fn reject_symlink_path(root: &Path, parent: Option<&Path>) -> Result<(), String> {
    let Some(parent) = parent else {
        return Ok(());
    };
    let mut current = root.to_path_buf();
    for component in parent.components() {
        let Component::Normal(segment) = component else {
            continue;
        };
        current.push(segment);
        match fs::symlink_metadata(current.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "generated config path crosses symlink: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => {
                return Err(format!(
                    "inspect generated config path {} failed: {err}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_config_paths_reject_escape_and_absolute_paths() {
        assert_eq!(
            normalize_generated_config_path(".chatos/runtime.env").expect("relative path"),
            PathBuf::from(".chatos/runtime.env")
        );
        assert!(normalize_generated_config_path("../runtime.env").is_err());
        assert!(normalize_generated_config_path("config/../runtime.env").is_err());
        assert!(normalize_generated_config_path("/tmp/runtime.env").is_err());
    }
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
