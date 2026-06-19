use std::collections::HashMap;

use super::path_utils::{is_path_within_scope, normalize_path_string, path_matches_root};
use super::snapshot::DirtyScopeSnapshot;
use super::{FileFingerprint, WorkspacePathChange};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DirtyPathScopeKind {
    Root,
    Project,
    Other,
}

pub(super) fn apply_dirty_scope_snapshots(
    files: &mut HashMap<String, FileFingerprint>,
    snapshots: Vec<DirtyScopeSnapshot>,
) -> Vec<WorkspacePathChange> {
    let scope_paths = snapshots
        .iter()
        .map(|snapshot| snapshot.scope_path.clone())
        .collect::<Vec<_>>();
    let previous_by_scope = take_scoped_previous_files(files, scope_paths.as_slice());

    let mut changes = Vec::new();
    for snapshot in &snapshots {
        let previous_scope = previous_by_scope
            .get(snapshot.scope_path.as_str())
            .cloned()
            .unwrap_or_default();
        changes.extend(diff_workspace_files(&previous_scope, &snapshot.files));
        files.extend(snapshot.files.clone());
    }
    changes
}

pub(super) fn diff_workspace_files(
    previous: &HashMap<String, FileFingerprint>,
    current: &HashMap<String, FileFingerprint>,
) -> Vec<WorkspacePathChange> {
    let mut changes = Vec::new();

    for (path, current_fp) in current {
        match previous.get(path) {
            Some(previous_fp) if previous_fp == current_fp => {}
            Some(_) => changes.push(WorkspacePathChange {
                path: path.clone(),
                kind: "edit",
                bytes: current_fp.size_bytes.min(i64::MAX as u64) as i64,
                signature: current_fp.signature(),
                fingerprint: Some(current_fp.clone()),
            }),
            None => changes.push(WorkspacePathChange {
                path: path.clone(),
                kind: "create",
                bytes: current_fp.size_bytes.min(i64::MAX as u64) as i64,
                signature: current_fp.signature(),
                fingerprint: Some(current_fp.clone()),
            }),
        }
    }

    for (path, previous_fp) in previous {
        if current.contains_key(path) {
            continue;
        }
        changes.push(WorkspacePathChange {
            path: path.clone(),
            kind: "delete",
            bytes: previous_fp.size_bytes.min(i64::MAX as u64) as i64,
            signature: previous_fp.signature(),
            fingerprint: None,
        });
    }

    changes
}

pub(super) fn collect_project_dirty_paths(
    dirty_paths: &[String],
    project_root: &str,
) -> Vec<String> {
    let normalized_root = normalize_path_string(project_root);
    if normalized_root.is_empty() {
        return Vec::new();
    }

    let relevant = dirty_paths
        .iter()
        .filter(|path| path_matches_root(path.as_str(), normalized_root.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    collapse_dirty_paths(relevant)
}

pub(super) fn classify_dirty_path_scope(
    project_root: &str,
    dirty_paths: &[String],
) -> DirtyPathScopeKind {
    if dirty_paths
        .iter()
        .any(|path| path == project_root || path_matches_root(project_root, path))
    {
        return DirtyPathScopeKind::Root;
    }
    if dirty_paths
        .iter()
        .any(|path| path_matches_root(path, project_root))
    {
        return DirtyPathScopeKind::Project;
    }
    DirtyPathScopeKind::Other
}

pub(super) fn collapse_dirty_paths(paths: Vec<String>) -> Vec<String> {
    let mut sorted = paths;
    sorted.sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));

    let mut collapsed = Vec::new();
    for path in sorted {
        if collapsed
            .iter()
            .any(|existing: &String| is_path_within_scope(path.as_str(), existing.as_str()))
        {
            continue;
        }
        collapsed.push(path);
    }
    collapsed
}

pub(super) fn take_scoped_previous_files(
    files: &mut HashMap<String, FileFingerprint>,
    scope_paths: &[String],
) -> HashMap<String, HashMap<String, FileFingerprint>> {
    let mut scoped = scope_paths
        .iter()
        .cloned()
        .map(|scope| (scope, HashMap::new()))
        .collect::<HashMap<_, _>>();
    let keys_to_remove = files
        .keys()
        .filter_map(|path| {
            matching_scope(path.as_str(), scope_paths)
                .map(|scope| (path.clone(), scope.to_string()))
        })
        .collect::<Vec<_>>();

    for (path, scope) in keys_to_remove {
        let Some(fingerprint) = files.remove(path.as_str()) else {
            continue;
        };
        scoped.entry(scope).or_default().insert(path, fingerprint);
    }

    scoped
}

fn matching_scope<'a>(path: &str, scope_paths: &'a [String]) -> Option<&'a str> {
    scope_paths
        .iter()
        .find(|scope| is_path_within_scope(path, scope.as_str()))
        .map(String::as_str)
}
