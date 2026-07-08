// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio::sync::Notify;
use tracing::{debug, warn};

use crate::builtin::code_maintainer::ChangeLogStore;
use crate::models::project::{Project, ProjectService};
use crate::services::project_fs_cache::invalidate_directory_listing_cache_for_path;
use crate::services::project_run::{classify_project_run_path_change, ProjectRunPathChangeKind};
use crate::services::realtime::publish_project_run_catalog_updated;

mod dirty_scope;
mod path_utils;
mod snapshot;

use dirty_scope::{
    apply_dirty_scope_snapshots, classify_dirty_path_scope, collect_project_dirty_paths,
    diff_workspace_files, DirtyPathScopeKind,
};
use path_utils::normalize_path_string;
use snapshot::{
    collect_dirty_scope_snapshots, collect_workspace_snapshot, current_file_fingerprint,
    DirtyScopeCollectResult, SnapshotCollectResult,
};

const WORKSPACE_WATCHER_SERVER_NAME: &str = "workspace_watcher";
const DEFAULT_WORKSPACE_WATCHER_FULL_SCAN_INTERVAL_SECS: u64 = 60;
const MIN_WORKSPACE_WATCHER_FULL_SCAN_INTERVAL_SECS: u64 = 5;
const WORKSPACE_WATCHER_MAX_INCREMENTAL_DIRTY_PATHS: usize = 64;
const WORKSPACE_WATCHER_IDLE_WAIT: Duration = Duration::from_secs(1);
const WORKSPACE_WATCHER_SUPPRESSION_TTL: Duration = Duration::from_secs(30);

static WATCHER_STATE: Lazy<Arc<WorkspaceRealtimeWatcherState>> =
    Lazy::new(|| Arc::new(WorkspaceRealtimeWatcherState::default()));
static WORKSPACE_WATCHER_FULL_SCAN_INTERVAL: Lazy<Option<Duration>> = Lazy::new(|| {
    let parsed = std::env::var("WORKSPACE_WATCHER_FULL_SCAN_INTERVAL_SECONDS")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u64>().ok());

    match parsed {
        Some(0) => None,
        Some(seconds) => Some(Duration::from_secs(
            seconds.max(MIN_WORKSPACE_WATCHER_FULL_SCAN_INTERVAL_SECS),
        )),
        None => Some(Duration::from_secs(
            DEFAULT_WORKSPACE_WATCHER_FULL_SCAN_INTERVAL_SECS,
        )),
    }
});

#[derive(Default)]
struct WorkspaceRealtimeWatcherState {
    started: Mutex<bool>,
    project_states: Mutex<HashMap<String, ProjectSnapshotState>>,
    dirty_paths: Mutex<HashSet<String>>,
    suppressed_paths: Mutex<HashMap<String, SuppressionEntry>>,
    notify: Notify,
}

#[derive(Clone, Default)]
struct ProjectSnapshotState {
    root_path: String,
    files: HashMap<String, FileFingerprint>,
    initialized: bool,
    root_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileFingerprint {
    modified_millis: u128,
    size_bytes: u64,
}

#[derive(Clone)]
struct SuppressionEntry {
    expectation: SuppressionExpectation,
    added_at: Instant,
}

#[derive(Clone)]
enum SuppressionExpectation {
    Present(FileFingerprint),
    Missing,
}

#[derive(Clone)]
struct WorkspacePathChange {
    path: String,
    kind: &'static str,
    bytes: i64,
    signature: String,
    fingerprint: Option<FileFingerprint>,
}

enum IncrementalScanOutcome {
    Handled(Vec<WorkspacePathChange>),
    FallbackToFullScan,
}

pub fn start_workspace_realtime_watcher() {
    let state = WATCHER_STATE.clone();
    let mut started = state.started.lock();
    if *started {
        return;
    }
    *started = true;
    tokio::spawn(async move {
        run_workspace_realtime_watcher().await;
    });
}

pub fn note_workspace_path_changed(path: &str) {
    let normalized = normalize_path_string(path);
    if normalized.is_empty() {
        return;
    }
    WATCHER_STATE.dirty_paths.lock().insert(normalized);
    WATCHER_STATE.notify.notify_one();
}

pub fn suppress_logged_path(path: &str) {
    let normalized = normalize_path_string(path);
    if normalized.is_empty() {
        return;
    }

    let expectation = current_file_fingerprint(Path::new(&normalized))
        .map(SuppressionExpectation::Present)
        .unwrap_or(SuppressionExpectation::Missing);

    WATCHER_STATE.suppressed_paths.lock().insert(
        normalized.clone(),
        SuppressionEntry {
            expectation,
            added_at: Instant::now(),
        },
    );
    WATCHER_STATE.dirty_paths.lock().insert(normalized);
    WATCHER_STATE.notify.notify_one();
}

async fn run_workspace_realtime_watcher() {
    let full_scan_interval = *WORKSPACE_WATCHER_FULL_SCAN_INTERVAL;
    let mut pending_initial_scan = true;
    let mut last_full_scan = full_scan_interval
        .and_then(|interval| Instant::now().checked_sub(interval))
        .unwrap_or_else(Instant::now);

    loop {
        let should_run_full_scan = pending_initial_scan
            || full_scan_interval
                .map(|interval| last_full_scan.elapsed() >= interval)
                .unwrap_or(false);
        let dirty_paths = take_dirty_paths();

        if should_run_full_scan || !dirty_paths.is_empty() {
            if should_run_full_scan {
                pending_initial_scan = false;
                last_full_scan = Instant::now();
            }
            if let Err(err) = scan_projects(should_run_full_scan, dirty_paths).await {
                warn!("workspace realtime watcher scan failed: {err}");
            }
            continue;
        }

        tokio::select! {
            _ = tokio::time::sleep(WORKSPACE_WATCHER_IDLE_WAIT) => {}
            _ = WATCHER_STATE.notify.notified() => {}
        }
    }
}

fn take_dirty_paths() -> Vec<String> {
    let mut guard = WATCHER_STATE.dirty_paths.lock();
    guard.drain().collect()
}

async fn scan_projects(full_scan: bool, dirty_paths: Vec<String>) -> Result<(), String> {
    prune_expired_suppressions();

    let projects = ProjectService::list(None).await?;
    let project_id_set: HashSet<String> = projects.iter().map(|item| item.id.clone()).collect();
    WATCHER_STATE
        .project_states
        .lock()
        .retain(|project_id, _| project_id_set.contains(project_id));

    for project in projects {
        let project_dirty_paths = if full_scan {
            Vec::new()
        } else {
            collect_project_dirty_paths(dirty_paths.as_slice(), project.root_path.as_str())
        };
        if !full_scan && project_dirty_paths.is_empty() {
            continue;
        }
        if let Err(err) = scan_project(&project, full_scan, project_dirty_paths).await {
            warn!(
                "workspace realtime watcher project scan failed: project_id={} root={} err={}",
                project.id, project.root_path, err,
            );
        }
    }

    Ok(())
}

async fn scan_project(
    project: &Project,
    full_scan: bool,
    dirty_paths: Vec<String>,
) -> Result<(), String> {
    let normalized_root = normalize_path_string(project.root_path.as_str());
    if normalized_root.is_empty() {
        return Ok(());
    }

    let dirty_scope_kind = classify_dirty_path_scope(&normalized_root, dirty_paths.as_slice());
    let should_run_full_scan = {
        let mut state_guard = WATCHER_STATE.project_states.lock();
        let state = state_guard
            .entry(project.id.clone())
            .or_insert_with(ProjectSnapshotState::default);
        if state.root_path != normalized_root {
            *state = ProjectSnapshotState {
                root_path: normalized_root.clone(),
                files: HashMap::new(),
                initialized: false,
                root_available: false,
            };
        }
        full_scan
            || !state.initialized
            || dirty_paths.len() > WORKSPACE_WATCHER_MAX_INCREMENTAL_DIRTY_PATHS
            || dirty_scope_kind == DirtyPathScopeKind::Root
    };

    if should_run_full_scan {
        return scan_project_full(project, normalized_root).await;
    }

    scan_project_incremental(project, normalized_root, dirty_paths).await
}

async fn scan_project_full(project: &Project, normalized_root: String) -> Result<(), String> {
    let snapshot = collect_workspace_snapshot(normalized_root.clone()).await?;

    let mut state_guard = WATCHER_STATE.project_states.lock();
    let state = state_guard
        .entry(project.id.clone())
        .or_insert_with(ProjectSnapshotState::default);
    if state.root_path != normalized_root {
        *state = ProjectSnapshotState {
            root_path: normalized_root,
            files: HashMap::new(),
            initialized: false,
            root_available: false,
        };
    }

    match snapshot {
        SnapshotCollectResult::RootMissing => {
            mark_project_root_missing(project, state);
            return Ok(());
        }
        SnapshotCollectResult::Ready(current_files) => {
            mark_project_root_available(project, state);

            if !state.initialized {
                state.files = current_files;
                state.initialized = true;
                return Ok(());
            }

            let previous_files = std::mem::take(&mut state.files);
            let changes = diff_workspace_files(&previous_files, &current_files);
            state.files = current_files;
            state.initialized = true;
            drop(state_guard);

            if changes.is_empty() {
                return Ok(());
            }

            process_workspace_changes(project, changes)?;
        }
    }

    Ok(())
}

async fn scan_project_incremental(
    project: &Project,
    normalized_root: String,
    dirty_paths: Vec<String>,
) -> Result<(), String> {
    let snapshots = collect_dirty_scope_snapshots(normalized_root.clone(), dirty_paths).await?;
    let outcome = {
        let mut state_guard = WATCHER_STATE.project_states.lock();
        let state = state_guard
            .entry(project.id.clone())
            .or_insert_with(ProjectSnapshotState::default);
        if state.root_path != normalized_root {
            *state = ProjectSnapshotState {
                root_path: normalized_root.clone(),
                files: HashMap::new(),
                initialized: false,
                root_available: false,
            };
        }

        match snapshots {
            DirtyScopeCollectResult::RootMissing => {
                mark_project_root_missing(project, state);
                IncrementalScanOutcome::Handled(Vec::new())
            }
            DirtyScopeCollectResult::Ready(snapshots) => {
                mark_project_root_available(project, state);
                if !state.initialized {
                    IncrementalScanOutcome::FallbackToFullScan
                } else {
                    let changes = apply_dirty_scope_snapshots(&mut state.files, snapshots);
                    state.initialized = true;
                    IncrementalScanOutcome::Handled(changes)
                }
            }
        }
    };

    match outcome {
        IncrementalScanOutcome::Handled(changes) => process_workspace_changes(project, changes),
        IncrementalScanOutcome::FallbackToFullScan => {
            scan_project_full(project, normalized_root).await
        }
    }
}

fn mark_project_root_missing(project: &Project, state: &mut ProjectSnapshotState) {
    if !state.root_available {
        return;
    }

    state.root_available = false;
    state.initialized = false;
    state.files.clear();
    publish_project_root_status(project, "project_root_missing");
}

fn mark_project_root_available(project: &Project, state: &mut ProjectSnapshotState) {
    if state.root_available {
        return;
    }

    state.root_available = true;
    publish_project_root_status(project, "project_root_available");
}

fn publish_project_root_status(project: &Project, reason: &'static str) {
    let Some(user_id) = project.user_id.as_deref() else {
        return;
    };

    publish_project_run_catalog_updated(
        user_id,
        project.id.as_str(),
        reason,
        Some(project.root_path.as_str()),
    );
}

fn process_workspace_changes(
    project: &Project,
    changes: Vec<WorkspacePathChange>,
) -> Result<(), String> {
    if changes.is_empty() {
        return Ok(());
    }

    let mut store: Option<ChangeLogStore> = None;
    let mut logged_change_count = 0usize;
    let mut project_run_change: Option<(ProjectRunPathChangeKind, String)> = None;

    for change in changes {
        if is_suppressed_path(change.path.as_str(), change.fingerprint.as_ref()) {
            continue;
        }
        let store = match store.as_ref() {
            Some(existing) => existing,
            None => {
                store = Some(ChangeLogStore::new(
                    WORKSPACE_WATCHER_SERVER_NAME,
                    Some(project.id.clone()),
                    None,
                )?);
                match store.as_ref() {
                    Some(existing) => existing,
                    None => return Err("workspace watcher store initialization failed".to_string()),
                }
            }
        };
        store.log_change(
            change.path.as_str(),
            "workspace_scan",
            change.kind,
            change.bytes,
            change.signature.as_str(),
            "",
            "",
            None,
        )?;
        logged_change_count += 1;
        let _ = invalidate_directory_listing_cache_for_path(
            project.root_path.as_str(),
            Path::new(change.path.as_str()),
        );

        if let Some(kind) =
            classify_project_run_path_change(change.path.as_str(), Some(change.kind))
        {
            match &project_run_change {
                Some((ProjectRunPathChangeKind::Catalog, _)) => {}
                Some((ProjectRunPathChangeKind::Environment, _))
                    if kind == ProjectRunPathChangeKind::Environment => {}
                _ => {
                    project_run_change = Some((kind, change.path.clone()));
                }
            }
        }
    }

    if logged_change_count == 0 {
        return Ok(());
    }

    if let Some((kind, path)) = project_run_change {
        let Some(user_id) = project.user_id.as_deref() else {
            return Ok(());
        };
        publish_project_run_catalog_updated(
            user_id,
            project.id.as_str(),
            kind.realtime_reason(),
            Some(path.as_str()),
        );
    }

    Ok(())
}

fn prune_expired_suppressions() {
    WATCHER_STATE
        .suppressed_paths
        .lock()
        .retain(|_, entry| entry.added_at.elapsed() < WORKSPACE_WATCHER_SUPPRESSION_TTL);
}

fn is_suppressed_path(path: &str, current: Option<&FileFingerprint>) -> bool {
    let mut guard = WATCHER_STATE.suppressed_paths.lock();
    let Some(entry) = guard.get(path).cloned() else {
        return false;
    };
    if entry.added_at.elapsed() >= WORKSPACE_WATCHER_SUPPRESSION_TTL {
        guard.remove(path);
        return false;
    }

    let matches = match (&entry.expectation, current) {
        (SuppressionExpectation::Missing, None) => true,
        (SuppressionExpectation::Present(expected), Some(actual)) => expected == actual,
        _ => false,
    };

    if matches {
        debug!(
            "workspace watcher suppressed duplicate path change: {}",
            path
        );
        guard.remove(path);
        return true;
    }
    false
}

impl FileFingerprint {
    fn signature(&self) -> String {
        format!(
            "workspace-scan:{}:{}",
            self.modified_millis, self.size_bytes
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::dirty_scope::{collapse_dirty_paths, take_scoped_previous_files};
    use super::path_utils::{is_path_within_scope, normalize_path_string, path_matches_root};
    use super::FileFingerprint;

    #[test]
    fn normalize_path_string_keeps_absolute_paths_stable() {
        let normalized = normalize_path_string("/tmp/demo/../demo/project");
        assert_eq!(normalized, "/tmp/demo/project");
    }

    #[test]
    fn path_matches_root_requires_same_root_scope() {
        assert!(path_matches_root("/tmp/demo/file.txt", "/tmp/demo"));
        assert!(!path_matches_root("/tmp/demo-two/file.txt", "/tmp/demo"));
    }

    #[test]
    fn collapse_dirty_paths_prefers_ancestor_scope() {
        let collapsed = collapse_dirty_paths(vec![
            "/tmp/demo/src/main.rs".to_string(),
            "/tmp/demo/src".to_string(),
            "/tmp/demo/src/lib.rs".to_string(),
            "/tmp/demo/README.md".to_string(),
        ]);
        assert_eq!(
            collapsed,
            vec![
                "/tmp/demo/src".to_string(),
                "/tmp/demo/README.md".to_string(),
            ]
        );
    }

    #[test]
    fn take_scoped_previous_files_removes_exact_and_nested_matches() {
        let mut files = HashMap::from([
            (
                "/tmp/demo/src/main.rs".to_string(),
                FileFingerprint {
                    modified_millis: 1,
                    size_bytes: 10,
                },
            ),
            (
                "/tmp/demo/src/lib.rs".to_string(),
                FileFingerprint {
                    modified_millis: 2,
                    size_bytes: 20,
                },
            ),
            (
                "/tmp/demo/README.md".to_string(),
                FileFingerprint {
                    modified_millis: 3,
                    size_bytes: 30,
                },
            ),
        ]);

        let scoped = take_scoped_previous_files(
            &mut files,
            &[
                "/tmp/demo/src".to_string(),
                "/tmp/demo/README.md".to_string(),
            ],
        );

        assert!(files.is_empty());
        assert_eq!(scoped["/tmp/demo/src"].len(), 2);
        assert_eq!(scoped["/tmp/demo/README.md"].len(), 1);
    }

    #[test]
    fn is_path_within_scope_respects_path_boundaries() {
        assert!(is_path_within_scope(
            "/tmp/demo/src/main.rs",
            "/tmp/demo/src"
        ));
        assert!(is_path_within_scope("/tmp/demo/src", "/tmp/demo/src"));
        assert!(!is_path_within_scope(
            "/tmp/demo/src-two/file.rs",
            "/tmp/demo/src"
        ));
    }
}
