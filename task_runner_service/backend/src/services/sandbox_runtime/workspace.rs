// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Component, Path, PathBuf};
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
    let source = fs::canonicalize(source)
        .map_err(|err| format!("read source workspace {source} failed: {err}"))?;
    let destination = PathBuf::from(destination);
    fs::create_dir_all(&destination).map_err(|err| {
        format!(
            "create sandbox run workspace {} failed: {err}",
            destination.display()
        )
    })?;
    clear_directory(destination.as_path())?;
    copy_directory_contents(source.as_path(), destination.as_path(), source.as_path())
}

fn clear_directory(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path)
        .map_err(|err| format!("read directory {} failed: {err}", path.display()))?
    {
        let entry = entry.map_err(|err| format!("read directory entry failed: {err}"))?;
        let entry_path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| format!("read metadata {} failed: {err}", entry_path.display()))?;
        if metadata.is_dir() {
            fs::remove_dir_all(&entry_path).map_err(|err| {
                format!("remove directory {} failed: {err}", entry_path.display())
            })?;
        } else {
            fs::remove_file(&entry_path)
                .map_err(|err| format!("remove file {} failed: {err}", entry_path.display()))?;
        }
    }
    Ok(())
}

fn copy_directory_contents(source: &Path, destination: &Path, root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|err| format!("read directory {} failed: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("read directory entry failed: {err}"))?;
        let source_path = entry.path();
        if should_skip_workspace_entry(root, source_path.as_path()) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| format!("read file type {} failed: {err}", source_path.display()))?;
        let dest_path = destination.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            fs::create_dir_all(&dest_path)
                .map_err(|err| format!("create directory {} failed: {err}", dest_path.display()))?;
            copy_directory_contents(source_path.as_path(), dest_path.as_path(), root)?;
        } else if file_type.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    format!("create directory {} failed: {err}", parent.display())
                })?;
            }
            fs::copy(&source_path, &dest_path).map_err(|err| {
                format!(
                    "copy file {} to {} failed: {err}",
                    source_path.display(),
                    dest_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn should_skip_workspace_entry(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative
        .components()
        .next()
        .is_some_and(|component| matches!(component, Component::Normal(name) if name == ".chatos"))
}
