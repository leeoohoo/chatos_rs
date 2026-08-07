// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, Weak};

type SharedRevisionGuard = Arc<Mutex<ModificationRevisionGuard>>;

pub(super) fn guard_for_workspace(root: &Path) -> Result<SharedRevisionGuard, String> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Weak<Mutex<ModificationRevisionGuard>>>>> =
        OnceLock::new();
    let workspace = root.to_string_lossy().to_string();
    let mut registry = REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "file revision guard registry unavailable".to_string())?;
    registry.retain(|_, guard| guard.strong_count() > 0);
    if let Some(guard) = registry.get(workspace.as_str()).and_then(Weak::upgrade) {
        return Ok(guard);
    }
    let guard = Arc::new(Mutex::new(ModificationRevisionGuard::default()));
    registry.insert(workspace, Arc::downgrade(&guard));
    Ok(guard)
}

#[derive(Debug, Default)]
pub(super) struct ModificationRevisionGuard {
    reread_required: HashSet<(String, String)>,
}

impl ModificationRevisionGuard {
    pub(super) fn record_read(&mut self, run_id: &str, path: &str) {
        self.reread_required
            .remove(&(run_id.to_string(), normalize_path(path)));
    }

    pub(super) fn require_reread(&mut self, run_id: &str, path: &str) {
        self.reread_required
            .insert((run_id.to_string(), normalize_path(path)));
    }

    pub(super) fn is_reread_required(&self, run_id: &str, path: &str) -> bool {
        self.reread_required
            .contains(&(run_id.to_string(), normalize_path(path)))
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

        guard.record_read("run-1", "src/lib.rs");

        assert!(!guard.is_reread_required("run-1", "src/lib.rs"));
        assert!(guard.is_reread_required("run-2", "src/lib.rs"));
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
}
