// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::config::path_depth_all_platforms;
use super::super::*;
use super::materialization::*;
use super::paths::canonical_target_if_symlinked_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::command_sandbox) struct MaterializedWritableRoot {
    pub(in crate::command_sandbox) logical: PathBuf,
    pub(in crate::command_sandbox) mount: PathBuf,
}

pub(in crate::command_sandbox) fn materialized_writable_roots(
    materialized: &MaterializedPermissions,
) -> Vec<MaterializedWritableRoot> {
    let mut roots = materialized
        .entries
        .iter()
        .filter(|entry| entry.access == FileSystemAccessMode::Write)
        .map(|entry| MaterializedWritableRoot {
            logical: entry.path.clone(),
            mount: canonical_target_if_symlinked_path(entry.path.as_path())
                .unwrap_or_else(|| entry.path.clone()),
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| {
        path_depth_all_platforms(left.logical.as_path())
            .cmp(&path_depth_all_platforms(right.logical.as_path()))
            .then_with(|| left.logical.cmp(&right.logical))
    });
    roots.dedup_by(|left, right| left.logical == right.logical);
    roots
}

pub(in crate::command_sandbox) fn allowed_write_paths(
    writable_roots: &[MaterializedWritableRoot],
) -> Vec<PathBuf> {
    let mut paths = writable_roots
        .iter()
        .flat_map(|root| [root.logical.clone(), root.mount.clone()])
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

pub(in crate::command_sandbox) fn remap_path_for_writable_root(
    path: &Path,
    writable_roots: &[MaterializedWritableRoot],
) -> PathBuf {
    let Some(root) = writable_roots
        .iter()
        .filter(|root| path.starts_with(root.logical.as_path()))
        .max_by_key(|root| path_depth_all_platforms(root.logical.as_path()))
    else {
        return path.to_path_buf();
    };
    if root.logical == root.mount {
        return path.to_path_buf();
    }
    path.strip_prefix(root.logical.as_path())
        .map(|relative| root.mount.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(in crate::command_sandbox) fn is_within_allowed_write_paths(
    path: &Path,
    allowed_write_paths: &[PathBuf],
) -> bool {
    allowed_write_paths
        .iter()
        .any(|root| path.starts_with(root.as_path()))
}

pub(in crate::command_sandbox) fn first_writable_symlink_component_in_path(
    target_path: &Path,
    allowed_write_paths: &[PathBuf],
) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for component in target_path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => continue,
            Component::ParentDir => {
                current.pop();
                continue;
            }
            Component::Normal(part) => current.push(part),
        }

        let metadata = match std::fs::symlink_metadata(current.as_path()) {
            Ok(metadata) => metadata,
            Err(_) => break,
        };
        if metadata.file_type().is_symlink()
            && is_within_allowed_write_paths(current.as_path(), allowed_write_paths)
        {
            return Some(current);
        }
    }
    None
}
