// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use serde_json::json;

use super::utils::{generate_id, now_iso};

type SharedEditSessionStore = Arc<Mutex<EditSessionStore>>;

pub(super) fn store_for_workspace(root: &Path) -> Result<SharedEditSessionStore, String> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Weak<Mutex<EditSessionStore>>>>> =
        OnceLock::new();
    let workspace = root.to_string_lossy().to_string();
    let mut registry = REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "edit session registry unavailable".to_string())?;
    registry.retain(|_, store| store.strong_count() > 0);
    if let Some(store) = registry.get(workspace.as_str()).and_then(Weak::upgrade) {
        return Ok(store);
    }
    let store = Arc::new(Mutex::new(EditSessionStore::default()));
    registry.insert(workspace, Arc::downgrade(&store));
    Ok(store)
}

#[derive(Debug, Default)]
pub(super) struct EditSessionStore {
    sessions: HashMap<String, EditSession>,
}

impl EditSessionStore {
    pub(super) fn open_session(
        &mut self,
        run_id: &str,
        conversation_id: &str,
    ) -> EditSessionHandle {
        self.prune_run(run_id);
        let session = EditSession::new(run_id, conversation_id);
        let handle = session.handle();
        self.sessions.insert(session.id.clone(), session);
        handle
    }

    pub(super) fn get_mut<'a>(
        &'a mut self,
        session_id: &str,
        run_id: &str,
    ) -> Result<&'a mut EditSession, String> {
        self.prune_run(run_id);
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("edit session not found: {session_id}"))?;
        if session.run_id != run_id {
            return Err(format!("edit session not found: {session_id}"));
        }
        Ok(session)
    }

    pub(super) fn take(
        &mut self,
        session_id: &str,
        run_id: &str,
    ) -> Result<EditSession, String> {
        self.prune_run(run_id);
        let session = self
            .sessions
            .remove(session_id)
            .ok_or_else(|| format!("edit session not found: {session_id}"))?;
        if session.run_id != run_id {
            return Err(format!("edit session not found: {session_id}"));
        }
        Ok(session)
    }

    fn prune_run(&mut self, run_id: &str) {
        self.sessions.retain(|_, session| session.run_id == run_id);
    }
}

#[derive(Debug, Clone)]
pub(super) struct EditSessionHandle {
    pub id: String,
    pub run_id: String,
    pub conversation_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub staged_operation_count: usize,
    pub staged_path_count: usize,
}

impl EditSessionHandle {
    pub(super) fn to_json(&self) -> serde_json::Value {
        json!({
            "session_id": self.id,
            "run_id": self.run_id,
            "conversation_id": self.conversation_id,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "staged_operation_count": self.staged_operation_count,
            "staged_path_count": self.staged_path_count,
        })
    }
}

#[derive(Debug)]
pub(super) struct EditSession {
    pub id: String,
    pub run_id: String,
    pub conversation_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub staged_operation_count: usize,
    pub files: BTreeMap<String, SessionFileState>,
}

impl EditSession {
    fn new(run_id: &str, conversation_id: &str) -> Self {
        let now = now_iso();
        Self {
            id: generate_id("edit_session"),
            run_id: run_id.to_string(),
            conversation_id: conversation_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
            staged_operation_count: 0,
            files: BTreeMap::new(),
        }
    }

    pub(super) fn touch(&mut self) {
        self.updated_at = now_iso();
    }

    pub(super) fn handle(&self) -> EditSessionHandle {
        EditSessionHandle {
            id: self.id.clone(),
            run_id: self.run_id.clone(),
            conversation_id: self.conversation_id.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            staged_operation_count: self.staged_operation_count,
            staged_path_count: self.changed_paths().len(),
        }
    }

    pub(super) fn changed_paths(&self) -> Vec<String> {
        self.files
            .values()
            .filter(|state| state.has_change())
            .map(|state| state.path.clone())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct SessionFileState {
    pub path: String,
    pub base: EntrySnapshot,
    pub working: EntrySnapshot,
    pub staged_operations: usize,
}

impl SessionFileState {
    pub(super) fn new(path: &str, snapshot: EntrySnapshot) -> Self {
        Self {
            path: normalize_path(path),
            base: snapshot.clone(),
            working: snapshot,
            staged_operations: 0,
        }
    }

    pub(super) fn has_change(&self) -> bool {
        self.base.kind != self.working.kind || self.base.content != self.working.content
    }

    pub(super) fn working_sha256(&self) -> Option<String> {
        self.working.sha256.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EntryKind {
    Missing,
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub(super) struct EntrySnapshot {
    pub kind: EntryKind,
    pub sha256: Option<String>,
    pub content: Option<String>,
}

impl EntrySnapshot {
    pub(super) fn missing() -> Self {
        Self {
            kind: EntryKind::Missing,
            sha256: None,
            content: None,
        }
    }

    pub(super) fn directory() -> Self {
        Self {
            kind: EntryKind::Directory,
            sha256: None,
            content: None,
        }
    }

    pub(super) fn file(content: String, sha256: String) -> Self {
        Self {
            kind: EntryKind::File,
            sha256: Some(sha256),
            content: Some(content),
        }
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}
