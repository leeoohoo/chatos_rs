// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

type SharedRevisionGuard = Arc<Mutex<ModificationRevisionGuard>>;

const REVISION_GUARD_TTL: Duration = Duration::from_secs(60 * 60);

pub(super) fn guard_for_workspace(root: &Path) -> Result<SharedRevisionGuard, String> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, SharedRevisionGuard>>> = OnceLock::new();
    let workspace = root.to_string_lossy().to_string();
    let mut registry = REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "file revision guard registry unavailable".to_string())?;
    registry.retain(|_, guard| {
        if Arc::strong_count(guard) > 1 {
            return true;
        }
        guard
            .lock()
            .map(|state| !state.is_expired())
            .unwrap_or(true)
    });
    if let Some(guard) = registry.get(workspace.as_str()) {
        return Ok(guard.clone());
    }
    let guard = Arc::new(Mutex::new(ModificationRevisionGuard::default()));
    registry.insert(workspace, guard.clone());
    Ok(guard)
}

#[derive(Debug)]
pub(super) struct ModificationRevisionGuard {
    reread_required: HashSet<(String, String)>,
    latest_reads: HashMap<(String, String), Option<String>>,
    last_touched: Instant,
}

impl Default for ModificationRevisionGuard {
    fn default() -> Self {
        Self {
            reread_required: HashSet::new(),
            latest_reads: HashMap::new(),
            last_touched: Instant::now(),
        }
    }
}

impl ModificationRevisionGuard {
    pub(super) fn record_read(&mut self, run_id: &str, path: &str, sha256: Option<&str>) {
        self.touch();
        let key = (run_id.to_string(), normalize_path(path));
        self.reread_required.remove(&key);
        self.latest_reads.insert(key, sha256.map(str::to_string));
    }

    pub(super) fn require_reread(&mut self, run_id: &str, path: &str) {
        self.touch();
        self.reread_required
            .insert((run_id.to_string(), normalize_path(path)));
    }

    pub(super) fn is_reread_required(&self, run_id: &str, path: &str) -> bool {
        self.reread_required
            .contains(&(run_id.to_string(), normalize_path(path)))
    }

    pub(super) fn latest_read_matches(
        &self,
        run_id: &str,
        path: &str,
        sha256: Option<&str>,
    ) -> bool {
        self.latest_reads
            .get(&(run_id.to_string(), normalize_path(path)))
            .is_some_and(|read_sha256| read_sha256.as_deref() == sha256)
    }

    fn touch(&mut self) {
        self.last_touched = Instant::now();
    }

    fn is_expired(&self) -> bool {
        self.last_touched.elapsed() >= REVISION_GUARD_TTL
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn successful_read_clears_only_the_same_run_and_path() {
        let mut guard = ModificationRevisionGuard::default();
        guard.require_reread("run-1", "src\\lib.rs");
        guard.require_reread("run-2", "src/lib.rs");

        guard.record_read("run-1", "src/lib.rs", Some("abc"));

        assert!(!guard.is_reread_required("run-1", "src/lib.rs"));
        assert!(guard.is_reread_required("run-2", "src/lib.rs"));
        assert!(guard.latest_read_matches("run-1", "src/lib.rs", Some("abc")));
        assert!(!guard.latest_read_matches("run-2", "src/lib.rs", Some("abc")));
    }

    #[test]
    fn services_for_the_same_workspace_share_revision_state() {
        let root = PathBuf::from(format!("/tmp/chatos-revision-guard-{}", std::process::id()));
        let first = guard_for_workspace(root.as_path()).expect("first guard");
        let second = guard_for_workspace(root.as_path()).expect("second guard");
        first
            .lock()
            .expect("first lock")
            .require_reread("run", "src/lib.rs");

        assert!(second
            .lock()
            .expect("second lock")
            .is_reread_required("run", "src/lib.rs"));
    }

    #[test]
    fn revision_state_survives_service_recreation() {
        let root = PathBuf::from(format!(
            "/tmp/chatos-revision-guard-recreated-{}",
            uuid::Uuid::new_v4()
        ));
        let first = guard_for_workspace(root.as_path()).expect("first guard");
        first
            .lock()
            .expect("first lock")
            .record_read("run", "src/lib.rs", Some("abc"));
        drop(first);

        let recreated = guard_for_workspace(root.as_path()).expect("recreated guard");
        assert!(recreated
            .lock()
            .expect("recreated lock")
            .latest_read_matches("run", "src/lib.rs", Some("abc")));
    }
}
