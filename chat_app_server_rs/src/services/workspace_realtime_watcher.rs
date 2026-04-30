use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio::sync::Notify;
use tracing::{debug, warn};
use walkdir::{DirEntry, WalkDir};

use crate::builtin::code_maintainer::ChangeLogStore;
use crate::models::project::{Project, ProjectService};
use crate::services::realtime::publish_project_run_catalog_updated;

const WORKSPACE_WATCHER_SERVER_NAME: &str = "workspace_watcher";
const WORKSPACE_WATCHER_FULL_SCAN_INTERVAL: Duration = Duration::from_secs(4);
const WORKSPACE_WATCHER_IDLE_WAIT: Duration = Duration::from_secs(1);
const WORKSPACE_WATCHER_SUPPRESSION_TTL: Duration = Duration::from_secs(30);
const WORKSPACE_RUNNER_SCRIPT_RELATIVE_PATH: &str = ".chatos/project_runner.sh";

static WATCHER_STATE: Lazy<Arc<WorkspaceRealtimeWatcherState>> =
    Lazy::new(|| Arc::new(WorkspaceRealtimeWatcherState::default()));

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
}

enum SnapshotCollectResult {
    Ready(HashMap<String, FileFingerprint>),
    RootMissing,
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
    let mut last_full_scan = Instant::now()
        .checked_sub(WORKSPACE_WATCHER_FULL_SCAN_INTERVAL)
        .unwrap_or_else(Instant::now);

    loop {
        let should_run_full_scan = last_full_scan.elapsed() >= WORKSPACE_WATCHER_FULL_SCAN_INTERVAL;
        let dirty_paths = take_dirty_paths();

        if should_run_full_scan || !dirty_paths.is_empty() {
            if should_run_full_scan {
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
        if !should_scan_project(full_scan, &dirty_paths, &project) {
            continue;
        }
        if let Err(err) = scan_project(&project).await {
            warn!(
                "workspace realtime watcher project scan failed: project_id={} root={} err={}",
                project.id,
                project.root_path,
                err,
            );
        }
    }

    Ok(())
}

fn should_scan_project(full_scan: bool, dirty_paths: &[String], project: &Project) -> bool {
    if full_scan {
        return true;
    }
    let root = normalize_path_string(project.root_path.as_str());
    if root.is_empty() {
        return false;
    }
    dirty_paths
        .iter()
        .any(|path| path_matches_root(path.as_str(), root.as_str()))
}

async fn scan_project(project: &Project) -> Result<(), String> {
    let normalized_root = normalize_path_string(project.root_path.as_str());
    if normalized_root.is_empty() {
        return Ok(());
    }

    let snapshot = collect_workspace_snapshot(normalized_root.clone()).await?;

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

    match snapshot {
        SnapshotCollectResult::RootMissing => {
            if state.root_available {
                state.root_available = false;
                state.initialized = false;
                state.files.clear();
                if let Some(user_id) = project.user_id.as_deref() {
                    publish_project_run_catalog_updated(
                        user_id,
                        project.id.as_str(),
                        "project_root_missing",
                        Some(project.root_path.as_str()),
                    );
                }
            }
            return Ok(());
        }
        SnapshotCollectResult::Ready(current_files) => {
            if !state.root_available {
                state.root_available = true;
                if let Some(user_id) = project.user_id.as_deref() {
                    publish_project_run_catalog_updated(
                        user_id,
                        project.id.as_str(),
                        "project_root_available",
                        Some(project.root_path.as_str()),
                    );
                }
            }

            if !state.initialized {
                state.files = current_files;
                state.initialized = true;
                return Ok(());
            }

            let previous_files = state.files.clone();
            let changes = diff_workspace_files(&previous_files, &current_files);
            state.files = current_files;
            state.initialized = true;
            drop(state_guard);

            if changes.is_empty() {
                return Ok(());
            }

            let store = ChangeLogStore::new(
                WORKSPACE_WATCHER_SERVER_NAME,
                Some(project.id.clone()),
                None,
            )?;
            let mut runner_script_changed = false;

            for change in changes {
                runner_script_changed = runner_script_changed
                    || is_runner_script_path(project.root_path.as_str(), change.path.as_str());
                let current_fingerprint = match change.kind {
                    "delete" => None,
                    _ => current_file_fingerprint(Path::new(change.path.as_str())),
                };
                if is_suppressed_path(change.path.as_str(), current_fingerprint.as_ref()) {
                    continue;
                }
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
            }

            if runner_script_changed {
                if let Some(user_id) = project.user_id.as_deref() {
                    let runner_script_path = Path::new(project.root_path.as_str())
                        .join(WORKSPACE_RUNNER_SCRIPT_RELATIVE_PATH)
                        .to_string_lossy()
                        .to_string();
                    publish_project_run_catalog_updated(
                        user_id,
                        project.id.as_str(),
                        "workspace_runner_script_changed",
                        Some(runner_script_path.as_str()),
                    );
                }
            }
        }
    }

    Ok(())
}

fn diff_workspace_files(
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
            }),
            None => changes.push(WorkspacePathChange {
                path: path.clone(),
                kind: "create",
                bytes: current_fp.size_bytes.min(i64::MAX as u64) as i64,
                signature: current_fp.signature(),
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
        });
    }

    changes
}

fn prune_expired_suppressions() {
    WATCHER_STATE.suppressed_paths.lock().retain(|_, entry| {
        entry.added_at.elapsed() < WORKSPACE_WATCHER_SUPPRESSION_TTL
    });
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
        debug!("workspace watcher suppressed duplicate path change: {}", path);
        guard.remove(path);
        return true;
    }
    false
}

async fn collect_workspace_snapshot(root: String) -> Result<SnapshotCollectResult, String> {
    tokio::task::spawn_blocking(move || collect_workspace_snapshot_blocking(root.as_str()))
        .await
        .map_err(|err| err.to_string())?
}

fn collect_workspace_snapshot_blocking(root: &str) -> Result<SnapshotCollectResult, String> {
    let root_path = PathBuf::from(root);
    if !root_path.exists() || !root_path.is_dir() {
        return Ok(SnapshotCollectResult::RootMissing);
    }

    let mut files = HashMap::new();
    for entry in WalkDir::new(&root_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_descend_into(entry, &root_path))
    {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        if entry.depth() == 0 || !entry.file_type().is_file() {
            continue;
        }

        let absolute_path = entry.path().to_path_buf();
        if should_ignore_file(absolute_path.as_path(), &root_path) {
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

    Ok(SnapshotCollectResult::Ready(files))
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
            | "project_runner"
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
    if normalized.starts_with("project_runner/") {
        return true;
    }
    false
}

fn current_file_fingerprint(path: &Path) -> Option<FileFingerprint> {
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

fn is_runner_script_path(project_root: &str, path: &str) -> bool {
    let project_root = normalize_path_string(project_root);
    if project_root.is_empty() {
        return false;
    }
    let runner_path = normalize_path_string(
        Path::new(project_root.as_str())
            .join(WORKSPACE_RUNNER_SCRIPT_RELATIVE_PATH)
            .to_string_lossy()
            .as_ref(),
    );
    runner_path == normalize_path_string(path)
}

fn path_matches_root(path: &str, root: &str) -> bool {
    if path == root {
        return true;
    }
    let prefix = if root.ends_with(std::path::MAIN_SEPARATOR) {
        root.to_string()
    } else {
        format!("{root}{}", std::path::MAIN_SEPARATOR)
    };
    path.starts_with(prefix.as_str())
}

fn normalize_relative_string(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
            _ => {}
        }
    }
    normalized.to_string_lossy().replace('\\', "/")
}

fn normalize_path_string(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Prefix(value) => normalized.push(value.as_os_str()),
            Component::RootDir => {
                let separator = std::path::MAIN_SEPARATOR.to_string();
                normalized.push(separator);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized.to_string_lossy().to_string()
}

impl FileFingerprint {
    fn signature(&self) -> String {
        format!("workspace-scan:{}:{}", self.modified_millis, self.size_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{is_runner_script_path, normalize_path_string, path_matches_root};

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
    fn runner_script_detection_matches_expected_relative_path() {
        assert!(is_runner_script_path(
            "/tmp/project",
            "/tmp/project/.chatos/project_runner.sh"
        ));
        assert!(!is_runner_script_path(
            "/tmp/project",
            "/tmp/project/project_runner/index.ts"
        ));
    }
}
