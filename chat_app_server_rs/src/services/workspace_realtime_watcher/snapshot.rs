use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use walkdir::{DirEntry, WalkDir};

use super::path_utils::{normalize_path_string, normalize_relative_string};
use super::FileFingerprint;
use crate::services::project_local_cache::is_project_runtime_relative_path;

pub(super) struct DirtyScopeSnapshot {
    pub(super) scope_path: String,
    pub(super) files: HashMap<String, FileFingerprint>,
}

pub(super) enum SnapshotCollectResult {
    Ready(HashMap<String, FileFingerprint>),
    RootMissing,
}

pub(super) enum DirtyScopeCollectResult {
    Ready(Vec<DirtyScopeSnapshot>),
    RootMissing,
}

pub(super) async fn collect_workspace_snapshot(
    root: String,
) -> Result<SnapshotCollectResult, String> {
    tokio::task::spawn_blocking(move || collect_workspace_snapshot_blocking(root.as_str()))
        .await
        .map_err(|err| err.to_string())?
}

pub(super) async fn collect_dirty_scope_snapshots(
    root: String,
    dirty_paths: Vec<String>,
) -> Result<DirtyScopeCollectResult, String> {
    tokio::task::spawn_blocking(move || {
        collect_dirty_scope_snapshots_blocking(root.as_str(), dirty_paths.as_slice())
    })
    .await
    .map_err(|err| err.to_string())?
}

fn collect_workspace_snapshot_blocking(root: &str) -> Result<SnapshotCollectResult, String> {
    let root_path = PathBuf::from(root);
    if !root_path.exists() || !root_path.is_dir() {
        return Ok(SnapshotCollectResult::RootMissing);
    }

    Ok(SnapshotCollectResult::Ready(collect_scope_files(
        root_path.as_path(),
        root_path.as_path(),
    )))
}

fn collect_dirty_scope_snapshots_blocking(
    root: &str,
    dirty_paths: &[String],
) -> Result<DirtyScopeCollectResult, String> {
    let root_path = PathBuf::from(root);
    if !root_path.exists() || !root_path.is_dir() {
        return Ok(DirtyScopeCollectResult::RootMissing);
    }

    let snapshots = dirty_paths
        .iter()
        .map(|scope_path| DirtyScopeSnapshot {
            scope_path: scope_path.clone(),
            files: collect_scope_files(root_path.as_path(), Path::new(scope_path)),
        })
        .collect();
    Ok(DirtyScopeCollectResult::Ready(snapshots))
}

fn collect_scope_files(root: &Path, scope: &Path) -> HashMap<String, FileFingerprint> {
    let Ok(metadata) = std::fs::symlink_metadata(scope) else {
        return HashMap::new();
    };
    if metadata.file_type().is_symlink() {
        return HashMap::new();
    }
    if metadata.is_file() {
        if should_ignore_file(scope, root) {
            return HashMap::new();
        }
        let Some(fingerprint) = current_file_fingerprint(scope) else {
            return HashMap::new();
        };
        return HashMap::from([(
            normalize_path_string(scope.to_string_lossy().as_ref()),
            fingerprint,
        )]);
    }
    if !metadata.is_dir() || should_ignore_directory(scope, root) {
        return HashMap::new();
    }

    let mut files = HashMap::new();
    for entry in WalkDir::new(scope)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_descend_into(entry, root))
    {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        if entry.depth() == 0 || !entry.file_type().is_file() {
            continue;
        }

        let absolute_path = entry.path().to_path_buf();
        if should_ignore_file(absolute_path.as_path(), root) {
            continue;
        }
        let Some(fingerprint) = current_file_fingerprint(absolute_path.as_path()) else {
            continue;
        };
        files.insert(
            normalize_path_string(absolute_path.to_string_lossy().as_ref()),
            fingerprint,
        );
    }

    files
}

fn should_descend_into(entry: &DirEntry, root: &Path) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if !entry.file_type().is_dir() {
        return true;
    }

    let Some(relative) = entry.path().strip_prefix(root).ok() else {
        return true;
    };
    let normalized = normalize_relative_string(relative);
    if normalized.is_empty() {
        return true;
    }
    if is_ignored_runtime_relative_path(normalized.as_str()) {
        return false;
    }

    !matches!(
        normalized.as_str(),
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".turbo"
            | ".idea"
            | ".vscode"
            | "coverage"
    )
}

fn should_ignore_directory(path: &Path, root: &Path) -> bool {
    let Some(relative) = path.strip_prefix(root).ok() else {
        return false;
    };
    let normalized = normalize_relative_string(relative);
    if normalized.is_empty() {
        return false;
    }
    if is_ignored_runtime_relative_path(normalized.as_str()) {
        return true;
    }

    matches!(
        normalized.as_str(),
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".turbo"
            | ".idea"
            | ".vscode"
            | "coverage"
    )
}

fn should_ignore_file(path: &Path, root: &Path) -> bool {
    let Some(relative) = path.strip_prefix(root).ok() else {
        return false;
    };
    let normalized = normalize_relative_string(relative);
    if normalized.is_empty() {
        return false;
    }
    is_ignored_runtime_relative_path(normalized.as_str())
}

fn is_ignored_runtime_relative_path(path: &str) -> bool {
    is_project_runtime_relative_path(path)
}

pub(super) fn current_file_fingerprint(path: &Path) -> Option<FileFingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified_millis = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or(0);
    Some(FileFingerprint {
        modified_millis,
        size_bytes: metadata.len(),
    })
}
