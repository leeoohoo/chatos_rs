// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub(super) fn clear_directory(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    }
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))
}

pub(super) fn copy_workspace_contents_to_sandbox(
    source: &Path,
    destination: &Path,
    root: &Path,
) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("create sandbox workspace {}", destination.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let source_path = entry.path();
        if should_skip_local_sandbox_copy(root, source_path.as_path()) {
            continue;
        }
        let file_type = entry.file_type()?;
        let destination_path = destination.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            copy_workspace_contents_to_sandbox(
                source_path.as_path(),
                destination_path.as_path(),
                root,
            )?;
        } else if file_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::copy(source_path.as_path(), destination_path.as_path()).with_context(|| {
                format!(
                    "copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn should_skip_local_sandbox_copy(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative.components().next().is_some_and(
        |component| matches!(component, std::path::Component::Normal(name) if name == ".chatos"),
    )
}
