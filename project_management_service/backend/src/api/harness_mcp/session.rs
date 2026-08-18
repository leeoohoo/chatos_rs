// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::json;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

pub(super) type SharedEditSessionStore = Arc<AsyncMutex<EditSessionStore>>;

const SESSION_TTL: Duration = Duration::from_secs(60 * 60);

pub(super) fn store_for_project(
    project_id: &str,
    repo_path: &str,
    branch_ref: &str,
) -> SharedEditSessionStore {
    static REGISTRY: OnceLock<Mutex<HashMap<String, SharedEditSessionStore>>> = OnceLock::new();
    let key = format!(
        "{}\0{}\0{}",
        project_id.trim(),
        repo_path.trim(),
        branch_ref.trim()
    );
    let mut registry = REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.retain(|_, store| {
        if Arc::strong_count(store) > 1 {
            return true;
        }
        match store.try_lock() {
            Ok(mut sessions) => {
                sessions.prune_expired();
                !sessions.is_empty()
            }
            Err(_) => true,
        }
    });
    if let Some(store) = registry.get(key.as_str()) {
        return store.clone();
    }
    let store = Arc::new(AsyncMutex::new(EditSessionStore::default()));
    registry.insert(key, store.clone());
    store
}

#[derive(Debug, Default)]
pub(super) struct EditSessionStore {
    sessions: HashMap<String, EditSession>,
}

impl EditSessionStore {
    pub(super) fn open_session(
        &mut self,
        project_id: &str,
        run_id: Option<&str>,
    ) -> EditSessionHandle {
        self.prune_expired();
        let session = EditSession::new(project_id, run_id);
        let handle = session.handle();
        self.sessions.insert(session.id.clone(), session);
        handle
    }

    pub(super) fn get_mut(
        &mut self,
        session_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<&mut EditSession, String> {
        self.prune_expired();
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("edit session not found: {session_id}"))?;
        session.ensure_owner(project_id, run_id)?;
        Ok(session)
    }

    pub(super) fn take(
        &mut self,
        session_id: &str,
        project_id: &str,
        run_id: Option<&str>,
    ) -> Result<EditSession, String> {
        self.prune_expired();
        let session = self
            .sessions
            .remove(session_id)
            .ok_or_else(|| format!("edit session not found: {session_id}"))?;
        if let Err(error) = session.ensure_owner(project_id, run_id) {
            self.sessions.insert(session.id.clone(), session);
            return Err(error);
        }
        Ok(session)
    }

    fn prune_expired(&mut self) {
        self.sessions
            .retain(|_, session| session.last_touched.elapsed() < SESSION_TTL);
    }

    fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(super) struct EditSessionHandle {
    pub(super) id: String,
    pub(super) project_id: String,
    pub(super) run_id: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) staged_operation_count: usize,
    pub(super) staged_path_count: usize,
}

impl EditSessionHandle {
    pub(super) fn to_json(&self) -> serde_json::Value {
        json!({
            "session_id": self.id,
            "project_id": self.project_id,
            "run_id": self.run_id,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "staged_operation_count": self.staged_operation_count,
            "staged_path_count": self.staged_path_count,
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct EditSession {
    pub(super) id: String,
    project_id: String,
    run_id: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    last_touched: Instant,
    pub(super) staged_operation_count: usize,
    pub(super) entries: BTreeMap<String, SessionEntryState>,
}

impl EditSession {
    fn new(project_id: &str, run_id: Option<&str>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: format!("edit_session_{}", Uuid::new_v4()),
            project_id: project_id.to_string(),
            run_id: run_id.map(ToOwned::to_owned),
            created_at: now.clone(),
            updated_at: now,
            last_touched: Instant::now(),
            staged_operation_count: 0,
            entries: BTreeMap::new(),
        }
    }

    fn ensure_owner(&self, project_id: &str, run_id: Option<&str>) -> Result<(), String> {
        if self.project_id == project_id && self.run_id.as_deref() == run_id {
            Ok(())
        } else {
            Err(format!("edit session not found: {}", self.id))
        }
    }

    pub(super) fn touch(&mut self) {
        self.updated_at = Utc::now().to_rfc3339();
        self.last_touched = Instant::now();
    }

    pub(super) fn handle(&self) -> EditSessionHandle {
        EditSessionHandle {
            id: self.id.clone(),
            project_id: self.project_id.clone(),
            run_id: self.run_id.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            staged_operation_count: self.staged_operation_count,
            staged_path_count: self.changed_entries().len(),
        }
    }

    pub(super) fn changed_entries(&self) -> Vec<&SessionEntryState> {
        self.entries
            .values()
            .filter(|state| state.has_change())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct SessionEntryState {
    pub(super) path: String,
    pub(super) base: EntrySnapshot,
    pub(super) working: EntrySnapshot,
    pub(super) staged_operations: usize,
}

impl SessionEntryState {
    pub(super) fn new(path: &str, snapshot: EntrySnapshot) -> Self {
        Self {
            path: path.to_string(),
            base: snapshot.clone(),
            working: snapshot,
            staged_operations: 0,
        }
    }

    pub(super) fn has_change(&self) -> bool {
        self.base != self.working
    }

    pub(super) fn working_sha256(&self) -> Option<&str> {
        match &self.working {
            EntrySnapshot::File(file) => Some(file.sha256.as_str()),
            EntrySnapshot::Missing | EntrySnapshot::Directory { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EntrySnapshot {
    Missing,
    File(FileSnapshot),
    Directory {
        files: BTreeMap<String, DirectoryFileSnapshot>,
    },
}

impl EntrySnapshot {
    pub(super) fn kind_name(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::File(_) => "file",
            Self::Directory { .. } => "directory",
        }
    }

    pub(super) fn sha256(&self) -> Option<&str> {
        match self {
            Self::File(file) => Some(file.sha256.as_str()),
            Self::Missing | Self::Directory { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileSnapshot {
    pub(super) content: String,
    pub(super) sha256: String,
    pub(super) harness_blob_sha: String,
    pub(super) size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DirectoryFileSnapshot {
    pub(super) sha256: String,
    pub(super) harness_blob_sha: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn project_store_survives_request_context_recreation() {
        let project_id = format!("project-{}", Uuid::new_v4());
        let repo_path = format!("repo-{}", Uuid::new_v4());
        let first_store = store_for_project(project_id.as_str(), repo_path.as_str(), "main");
        let session_id = first_store
            .lock()
            .await
            .open_session(project_id.as_str(), Some("run-a"))
            .id;
        drop(first_store);

        let second_store = store_for_project(project_id.as_str(), repo_path.as_str(), "main");
        assert!(second_store
            .lock()
            .await
            .get_mut(session_id.as_str(), project_id.as_str(), Some("run-a"))
            .is_ok());
    }
}
