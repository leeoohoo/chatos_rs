use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use uuid::Uuid;

const MAXIMUM_WRITE_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_OPERATIONS_PER_BATCH: usize = 200;
const EDIT_SESSION_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct EditScope {
    pub(super) owner_user_id: String,
    pub(super) workspace_scope_id: String,
    pub(super) conversation_id: String,
    pub(super) run_id: String,
}

#[derive(Debug)]
pub(super) struct CodeWriteError {
    pub(super) message: String,
    pub(super) known_pre_effect: bool,
}

impl CodeWriteError {
    fn rejected(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            known_pre_effect: true,
        }
    }

    fn commit_failed() -> Self {
        Self {
            message: "The staged project edit could not be committed safely.".into(),
            known_pre_effect: false,
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct CodeWriteStore {
    inner: Arc<Mutex<StoreState>>,
}

#[derive(Default)]
struct StoreState {
    sessions: HashMap<String, EditSession>,
    session_by_scope: HashMap<EditScope, String>,
    evidence_by_scope: HashMap<EditScope, ProjectWriteEvidence>,
}

struct ProjectWriteEvidence {
    readback_after_commit: bool,
    recorded_at: Instant,
}

#[derive(Clone)]
struct EditSession {
    id: String,
    scope: EditScope,
    project_root: PathBuf,
    owned_paths: Vec<String>,
    purpose: Option<String>,
    files: HashMap<String, FileState>,
    staged_operation_count: usize,
    opened_at: Instant,
}

#[derive(Clone)]
struct FileState {
    path: String,
    base: FileSnapshot,
    working: FileSnapshot,
    staged_operations: usize,
}

impl FileState {
    fn has_changes(&self) -> bool {
        self.base != self.working
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileSnapshot {
    Missing,
    Directory,
    File(String),
}

impl FileSnapshot {
    fn sha256(&self) -> Option<String> {
        match self {
            Self::File(content) => Some(format!("{:x}", Sha256::digest(content.as_bytes()))),
            Self::Missing | Self::Directory => None,
        }
    }

    fn content(&self) -> Option<&str> {
        match self {
            Self::File(content) => Some(content),
            Self::Missing | Self::Directory => None,
        }
    }
}

enum ExpectedSha {
    Missing,
    Hash(String),
}

impl ExpectedSha {
    fn matches(&self, snapshot: &FileSnapshot) -> bool {
        match (self, snapshot) {
            (Self::Missing, FileSnapshot::Missing) => true,
            (Self::Hash(expected), FileSnapshot::File(_)) => {
                snapshot.sha256().as_deref() == Some(expected)
            }
            _ => false,
        }
    }
}

impl CodeWriteStore {
    pub(super) fn call(
        &self,
        name: &str,
        arguments: &Map<String, Value>,
        scope: EditScope,
        project_root: &Path,
        owned_paths: &[String],
    ) -> Result<Value, CodeWriteError> {
        let canonical_root = fs::canonicalize(project_root)
            .map_err(|_| CodeWriteError::rejected("The project edit session is unavailable."))?;
        if !canonical_root.is_dir() {
            return Err(CodeWriteError::rejected(
                "The project edit session is unavailable.",
            ));
        }
        let mut state = self.lock();
        state.prune_expired();
        if name != "open_edit_session" {
            let session_id = required_string(arguments, "session_id")?;
            let session = state
                .sessions
                .get(session_id)
                .filter(|session| session.scope == scope)
                .ok_or_else(session_unavailable)?;
            if session.project_root != canonical_root || session.owned_paths != owned_paths {
                return Err(session_unavailable());
            }
        }
        match name {
            "open_edit_session" => state.open(arguments, scope, canonical_root, owned_paths),
            "stage_edit_batch" => state.stage(arguments, &scope),
            "commit_edit_session" => state.commit(arguments, &scope),
            "abort_edit_session" => state.abort(arguments, &scope),
            _ => Err(CodeWriteError::rejected(
                "The requested project edit tool is unavailable.",
            )),
        }
    }

    pub(super) fn release_run(&self, run_id: &str) {
        let mut state = self.lock();
        let ids = state
            .sessions
            .values()
            .filter(|session| session.scope.run_id == run_id)
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            state.remove_session(&id);
        }
    }

    pub(super) fn reset(&self) {
        let mut state = self.lock();
        state.sessions.clear();
        state.session_by_scope.clear();
        state.evidence_by_scope.clear();
    }

    pub(super) fn record_project_read(&self, scope: &EditScope) {
        let mut state = self.lock();
        state.prune_expired();
        if let Some(evidence) = state.evidence_by_scope.get_mut(scope) {
            evidence.readback_after_commit = true;
        }
    }

    pub(super) fn completion_verified(&self, owner_user_id: &str, run_id: &str) -> bool {
        let mut state = self.lock();
        state.prune_expired();
        state.evidence_by_scope.iter().any(|(scope, evidence)| {
            scope.owner_user_id == owner_user_id
                && scope.run_id == run_id
                && evidence.readback_after_commit
        })
    }

    fn lock(&self) -> MutexGuard<'_, StoreState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl StoreState {
    fn open(
        &mut self,
        arguments: &Map<String, Value>,
        scope: EditScope,
        project_root: PathBuf,
        owned_paths: &[String],
    ) -> Result<Value, CodeWriteError> {
        let fresh = optional_bool(arguments, "fresh")?.unwrap_or(false);
        let purpose = optional_string(arguments, "purpose", 8_000)?;
        if !fresh {
            if let Some(existing_id) = self.session_by_scope.get(&scope) {
                let existing = self
                    .sessions
                    .get(existing_id)
                    .ok_or_else(session_unavailable)?;
                if existing.project_root != project_root || existing.owned_paths != owned_paths {
                    return Err(session_unavailable());
                }
                return Ok(session_result(existing, true));
            }
        }
        if let Some(existing_id) = self.session_by_scope.get(&scope).cloned() {
            self.remove_session(&existing_id);
        }
        let id = Uuid::new_v4().to_string();
        let session = EditSession {
            id: id.clone(),
            scope: scope.clone(),
            project_root,
            owned_paths: owned_paths.to_vec(),
            purpose,
            files: HashMap::new(),
            staged_operation_count: 0,
            opened_at: Instant::now(),
        };
        let result = session_result(&session, false);
        self.sessions.insert(id.clone(), session);
        self.session_by_scope.insert(scope, id);
        Ok(result)
    }

    fn stage(
        &mut self,
        arguments: &Map<String, Value>,
        scope: &EditScope,
    ) -> Result<Value, CodeWriteError> {
        let session_id = required_string(arguments, "session_id")?;
        let operations = arguments
            .get("operations")
            .and_then(Value::as_array)
            .filter(|operations| {
                !operations.is_empty() && operations.len() <= MAXIMUM_OPERATIONS_PER_BATCH
            })
            .ok_or_else(|| invalid_arguments("operations must contain between 1 and 200 items"))?;
        let current = self
            .sessions
            .get(session_id)
            .filter(|session| &session.scope == scope)
            .cloned()
            .ok_or_else(session_unavailable)?;
        let mut session = current;
        let mut changed_paths = HashSet::new();
        let mut matches = Vec::new();
        for operation in operations {
            let operation = operation
                .as_object()
                .ok_or_else(|| invalid_arguments("operations contains an invalid item"))?;
            let result = apply_operation(operation, &mut session)?;
            if result.changed {
                changed_paths.insert(result.path.clone());
            }
            if let Some(match_result) = result.match_result {
                matches.push(json!({"path": result.path, "match": match_result}));
            }
        }
        session.staged_operation_count += operations.len();
        let mut pending = session
            .files
            .values()
            .filter(|file| file.has_changes())
            .collect::<Vec<_>>();
        pending.sort_by(|left, right| left.path.cmp(&right.path));
        let pending_paths = pending
            .iter()
            .map(|file| path_summary(file))
            .collect::<Vec<_>>();
        let mut batch_paths = changed_paths.into_iter().collect::<Vec<_>>();
        batch_paths.sort();
        let changed = !batch_paths.is_empty();
        let staged_operation_count = session.staged_operation_count;
        self.sessions.insert(session_id.to_owned(), session);
        Ok(json!({
            "outcome": if changed { "applied" } else { "already_applied" },
            "changed": changed,
            "changed_target_count": batch_paths.len(),
            "result": {
                "session_id": session_id,
                "staged_operation_count": staged_operation_count,
                "batch_operation_count": operations.len(),
                "batch_changed_paths": batch_paths,
                "pending_target_count": pending_paths.len(),
                "pending_paths": pending_paths,
            },
            "matches": matches,
        }))
    }

    fn commit(
        &mut self,
        arguments: &Map<String, Value>,
        scope: &EditScope,
    ) -> Result<Value, CodeWriteError> {
        let session_id = required_string(arguments, "session_id")?;
        let session = self
            .sessions
            .get(session_id)
            .filter(|session| &session.scope == scope)
            .cloned()
            .ok_or_else(session_unavailable)?;
        let mut changed = session
            .files
            .values()
            .filter(|file| file.has_changes())
            .cloned()
            .collect::<Vec<_>>();
        changed.sort_by(|left, right| left.path.cmp(&right.path));
        for file in &changed {
            validate_ownership(&file.path, &session.owned_paths)?;
        }
        validate_current_baselines(&changed, &session.project_root)?;
        if changed.is_empty() {
            self.evidence_by_scope.insert(
                session.scope.clone(),
                ProjectWriteEvidence {
                    readback_after_commit: false,
                    recorded_at: Instant::now(),
                },
            );
            self.remove_session(session_id);
            return Ok(json!({
                "outcome": "already_applied",
                "changed": false,
                "changed_target_count": 0,
                "result": {
                    "session_id": session_id,
                    "committed_paths": [],
                    "staged_operation_count": session.staged_operation_count,
                }
            }));
        }
        apply_transaction(&changed, &session.project_root)?;
        self.evidence_by_scope.insert(
            session.scope.clone(),
            ProjectWriteEvidence {
                readback_after_commit: false,
                recorded_at: Instant::now(),
            },
        );
        self.remove_session(session_id);
        Ok(json!({
            "outcome": "applied",
            "changed": true,
            "changed_target_count": changed.len(),
            "result": {
                "session_id": session_id,
                "committed_paths": changed.iter().map(|file| file.path.clone()).collect::<Vec<_>>(),
                "staged_operation_count": session.staged_operation_count,
            },
            "files": changed.iter().map(|file| json!({
                "path": file.path,
                "change_kind": change_kind(file),
                "deleted": matches!(file.working, FileSnapshot::Missing),
            })).collect::<Vec<_>>(),
        }))
    }

    fn abort(
        &mut self,
        arguments: &Map<String, Value>,
        scope: &EditScope,
    ) -> Result<Value, CodeWriteError> {
        let session_id = required_string(arguments, "session_id")?;
        let session = self
            .sessions
            .get(session_id)
            .filter(|session| &session.scope == scope)
            .cloned()
            .ok_or_else(session_unavailable)?;
        let discarded = session
            .files
            .values()
            .filter(|file| file.has_changes())
            .count();
        self.remove_session(session_id);
        Ok(json!({
            "outcome": "already_applied",
            "changed": false,
            "changed_target_count": 0,
            "result": {
                "session_id": session_id,
                "discarded_target_count": discarded,
                "staged_operation_count": session.staged_operation_count,
            }
        }))
    }

    fn prune_expired(&mut self) {
        let expired = self
            .sessions
            .values()
            .filter(|session| session.opened_at.elapsed() >= EDIT_SESSION_TTL)
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            self.remove_session(&id);
        }
        self.evidence_by_scope
            .retain(|_, evidence| evidence.recorded_at.elapsed() < EDIT_SESSION_TTL);
    }

    fn remove_session(&mut self, session_id: &str) {
        let Some(session) = self.sessions.remove(session_id) else {
            return;
        };
        if self
            .session_by_scope
            .get(&session.scope)
            .map(String::as_str)
            == Some(session_id)
        {
            self.session_by_scope.remove(&session.scope);
        }
    }
}

struct StageResult {
    path: String,
    changed: bool,
    match_result: Option<Value>,
}

fn apply_operation(
    operation: &Map<String, Value>,
    session: &mut EditSession,
) -> Result<StageResult, CodeWriteError> {
    let kind = required_string(operation, "kind")?;
    let path = normalize_path(required_string(operation, "path")?)?;
    validate_ownership(&path, &session.owned_paths)?;
    validate_no_path_overlap(&path, kind, &session.files)?;
    let mut state = state_for_operation(&path, operation, session)?;
    let match_result = match kind {
        "write" => {
            let content = required_raw_string(operation, "content")?;
            validate_write_size(content)?;
            if matches!(state.working, FileSnapshot::Directory) {
                return Err(invalid_arguments("the target is a directory"));
            }
            state.working = FileSnapshot::File(content.to_owned());
            None
        }
        "append" => {
            let content = required_raw_string(operation, "content")?;
            let current = match &state.working {
                FileSnapshot::File(content) => content.as_str(),
                FileSnapshot::Missing => "",
                FileSnapshot::Directory => {
                    return Err(invalid_arguments("the target is a directory"));
                }
            };
            let next = format!("{current}{content}");
            validate_write_size(&next)?;
            state.working = FileSnapshot::File(next);
            None
        }
        "replace_text" => {
            let FileSnapshot::File(content) = &state.working else {
                return Err(invalid_arguments(
                    "replace_text requires an existing text file",
                ));
            };
            let old_text = required_raw_string(operation, "old_text")?;
            let new_text = required_raw_string(operation, "new_text")?;
            let start_line = optional_usize(operation, "start_line")?;
            let end_line = optional_usize(operation, "end_line")?;
            if start_line.is_some() != end_line.is_some() {
                return Err(invalid_arguments(
                    "start_line and end_line must be provided together",
                ));
            }
            let expected_matches = optional_usize(operation, "expected_matches")?.unwrap_or(1);
            if expected_matches == 0 {
                return Err(invalid_arguments("expected_matches must be positive"));
            }
            let (replacement, actual) = replace_text(
                content,
                old_text,
                new_text,
                start_line,
                end_line,
                expected_matches,
            )?;
            validate_write_size(&replacement)?;
            let changed = replacement != *content;
            state.working = FileSnapshot::File(replacement);
            Some(json!({"match_count": actual, "changed": changed}))
        }
        "delete" => {
            if matches!(state.working, FileSnapshot::Directory) {
                return Err(invalid_arguments("directory deletion is not supported"));
            }
            state.working = FileSnapshot::Missing;
            None
        }
        _ => return Err(invalid_arguments("the edit operation kind is unsupported")),
    };
    state.staged_operations += 1;
    let changed = state.has_changes();
    session.files.insert(path.clone(), state);
    Ok(StageResult {
        path,
        changed,
        match_result,
    })
}

fn state_for_operation(
    path: &str,
    operation: &Map<String, Value>,
    session: &EditSession,
) -> Result<FileState, CodeWriteError> {
    if let Some(state) = session.files.get(path) {
        if operation.contains_key("expected_sha256") {
            let expected = expected_sha(operation)?;
            if !expected.matches(&state.base) && !expected.matches(&state.working) {
                return Err(stale_context(path));
            }
        }
        return Ok(state.clone());
    }
    let snapshot = snapshot(path, &session.project_root)?;
    let no_op_write = operation.get("kind").and_then(Value::as_str) == Some("write")
        && operation.get("content").and_then(Value::as_str) == snapshot.content();
    if !no_op_write {
        if !operation.contains_key("expected_sha256") {
            return Err(invalid_arguments(&format!(
                "expected_sha256 is required before the first edit to {path}"
            )));
        }
        if !expected_sha(operation)?.matches(&snapshot) {
            return Err(stale_context(path));
        }
    }
    Ok(FileState {
        path: path.to_owned(),
        base: snapshot.clone(),
        working: snapshot,
        staged_operations: 0,
    })
}

fn expected_sha(operation: &Map<String, Value>) -> Result<ExpectedSha, CodeWriteError> {
    match operation.get("expected_sha256") {
        Some(Value::Null) => Ok(ExpectedSha::Missing),
        Some(Value::String(value))
            if value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
        {
            Ok(ExpectedSha::Hash(value.clone()))
        }
        _ => Err(invalid_arguments("expected_sha256 is invalid")),
    }
}

fn snapshot(path: &str, root: &Path) -> Result<FileSnapshot, CodeWriteError> {
    let target = resolve_path(root, path)?;
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileSnapshot::Missing)
        }
        Err(_) => return Err(invalid_arguments("the project target is unavailable")),
    };
    if metadata.file_type().is_symlink() {
        return Err(invalid_arguments(
            "symbolic-link edit targets are not supported",
        ));
    }
    if metadata.is_dir() {
        return Ok(FileSnapshot::Directory);
    }
    if !metadata.is_file() || metadata.len() > MAXIMUM_WRITE_BYTES as u64 {
        return Err(invalid_arguments(
            "the project file is unavailable or too large",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(target)
        .and_then(|file| {
            file.take(MAXIMUM_WRITE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| invalid_arguments("the project file is unavailable"))?;
    if bytes.len() > MAXIMUM_WRITE_BYTES || bytes.iter().take(8_000).any(|byte| *byte == 0) {
        return Err(invalid_arguments(
            "binary or oversized files cannot be edited",
        ));
    }
    String::from_utf8(bytes)
        .map(FileSnapshot::File)
        .map_err(|_| invalid_arguments("the project file is not UTF-8 text"))
}

fn validate_current_baselines(states: &[FileState], root: &Path) -> Result<(), CodeWriteError> {
    let mut conflicts = Vec::new();
    for state in states {
        if snapshot(&state.path, root)? != state.base {
            conflicts.push(state.path.as_str());
        }
    }
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(CodeWriteError::rejected(format!(
            "The project changed after it was read; reopen the edit session: {}.",
            conflicts.join(", ")
        )))
    }
}

struct PreparedChange {
    target: PathBuf,
    temporary: Option<PathBuf>,
    backup: Option<PathBuf>,
}

fn apply_transaction(states: &[FileState], root: &Path) -> Result<(), CodeWriteError> {
    validate_current_baselines(states, root)?;
    let mut created_directories = Vec::new();
    let mut prepared = Vec::with_capacity(states.len());
    for state in states {
        let target = resolve_path(root, &state.path)?;
        let parent = target.parent().ok_or_else(CodeWriteError::commit_failed)?;
        create_safe_directories(root, parent, &mut created_directories)?;
        let temporary = match &state.working {
            FileSnapshot::File(content) => {
                let temporary = unique_sibling(&target, "chatos-edit");
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary);
                let Ok(mut file) = file else {
                    cleanup_prepared(&prepared, &created_directories);
                    return Err(CodeWriteError::commit_failed());
                };
                if file.write_all(content.as_bytes()).is_err() || file.sync_all().is_err() {
                    let _ = fs::remove_file(&temporary);
                    cleanup_prepared(&prepared, &created_directories);
                    return Err(CodeWriteError::commit_failed());
                }
                Some(temporary)
            }
            FileSnapshot::Missing => None,
            FileSnapshot::Directory => {
                cleanup_prepared(&prepared, &created_directories);
                return Err(CodeWriteError::commit_failed());
            }
        };
        prepared.push(PreparedChange {
            target,
            temporary,
            backup: None,
        });
    }

    for (index, state) in states.iter().enumerate() {
        let current = match snapshot(&state.path, root) {
            Ok(current) => current,
            Err(_) => {
                rollback(&mut prepared, index, &created_directories);
                return Err(CodeWriteError::commit_failed());
            }
        };
        if current != state.base {
            rollback(&mut prepared, index, &created_directories);
            return Err(CodeWriteError::rejected(
                "The project changed while the staged edit was being committed.",
            ));
        }
        if prepared[index].target.exists() {
            let backup = unique_sibling(&prepared[index].target, "chatos-backup");
            if fs::rename(&prepared[index].target, &backup).is_err() {
                rollback(&mut prepared, index, &created_directories);
                return Err(CodeWriteError::commit_failed());
            }
            prepared[index].backup = Some(backup);
        }
        if let Some(temporary) = prepared[index].temporary.as_ref() {
            if fs::rename(temporary, &prepared[index].target).is_err() {
                if let Some(backup) = prepared[index].backup.as_ref() {
                    let _ = fs::rename(backup, &prepared[index].target);
                }
                rollback(&mut prepared, index, &created_directories);
                return Err(CodeWriteError::commit_failed());
            }
        }
    }
    for change in &prepared {
        if let Some(backup) = &change.backup {
            let _ = fs::remove_file(backup);
        }
    }
    Ok(())
}

fn rollback(
    prepared: &mut [PreparedChange],
    applied_count: usize,
    created_directories: &[PathBuf],
) {
    for change in prepared.iter().take(applied_count).rev() {
        if change.target.exists() {
            let _ = fs::remove_file(&change.target);
        }
        if let Some(backup) = &change.backup {
            let _ = fs::rename(backup, &change.target);
        }
    }
    cleanup_prepared(prepared, created_directories);
}

fn cleanup_prepared(prepared: &[PreparedChange], created_directories: &[PathBuf]) {
    for change in prepared {
        if let Some(temporary) = &change.temporary {
            let _ = fs::remove_file(temporary);
        }
    }
    for directory in created_directories.iter().rev() {
        let _ = fs::remove_dir(directory);
    }
}

fn create_safe_directories(
    root: &Path,
    parent: &Path,
    created: &mut Vec<PathBuf>,
) -> Result<(), CodeWriteError> {
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| CodeWriteError::commit_failed())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(CodeWriteError::commit_failed());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(CodeWriteError::commit_failed())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|_| CodeWriteError::commit_failed())?;
                created.push(current.clone());
            }
            Err(_) => return Err(CodeWriteError::commit_failed()),
        }
    }
    Ok(())
}

fn resolve_path(root: &Path, relative: &str) -> Result<PathBuf, CodeWriteError> {
    let relative = normalize_path(relative)?;
    let canonical_root =
        fs::canonicalize(root).map_err(|_| invalid_arguments("the project root is unavailable"))?;
    let mut current = canonical_root.clone();
    let components = Path::new(&relative).components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(component) = component else {
            return Err(invalid_arguments("the project path is invalid"));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(invalid_arguments("symbolic-link paths cannot be edited"));
            }
            Ok(metadata) if index + 1 < components.len() && !metadata.is_dir() => {
                return Err(invalid_arguments(
                    "a project path parent is not a directory",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(invalid_arguments("the project path is unavailable")),
        }
    }
    if !current.starts_with(&canonical_root) {
        return Err(invalid_arguments(
            "the project path escapes the project root",
        ));
    }
    Ok(current)
}

fn normalize_path(raw: &str) -> Result<String, CodeWriteError> {
    let first = raw.split('/').next().unwrap_or_default().as_bytes();
    let drive_prefixed = first.len() >= 2 && first[1] == b':' && first[0].is_ascii_alphabetic();
    if raw.is_empty()
        || raw.len() > 4_096
        || raw.trim() != raw
        || raw.starts_with('/')
        || raw.starts_with('~')
        || raw.contains('\\')
        || raw.contains('\0')
        || drive_prefixed
        || raw
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(invalid_arguments(
            "paths must be normalized project-relative paths",
        ));
    }
    Ok(raw.to_owned())
}

fn validate_ownership(path: &str, owned_paths: &[String]) -> Result<(), CodeWriteError> {
    if owned_paths.is_empty()
        || owned_paths
            .iter()
            .any(|owned| path == owned || path.starts_with(&format!("{owned}/")))
    {
        Ok(())
    } else {
        Err(invalid_arguments(
            "the edit target is outside this Task's owned_paths",
        ))
    }
}

fn validate_no_path_overlap(
    path: &str,
    kind: &str,
    files: &HashMap<String, FileState>,
) -> Result<(), CodeWriteError> {
    for (existing_path, existing) in files {
        if existing_path != path
            && (path.starts_with(&format!("{existing_path}/"))
                || existing_path.starts_with(&format!("{path}/")))
            && (kind == "delete" || matches!(existing.working, FileSnapshot::Missing))
        {
            return Err(invalid_arguments(
                "one edit session cannot delete overlapping project paths",
            ));
        }
    }
    Ok(())
}

fn replace_text(
    content: &str,
    old_text: &str,
    new_text: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    expected_matches: usize,
) -> Result<(String, usize), CodeWriteError> {
    if old_text.is_empty() {
        return Err(invalid_arguments("old_text cannot be empty"));
    }
    let (start, end) = line_byte_range(content, start_line, end_line)?;
    let ranges = content[start..end]
        .match_indices(old_text)
        .map(|(offset, _)| start + offset..start + offset + old_text.len())
        .collect::<Vec<_>>();
    if ranges.len() != expected_matches {
        return Err(CodeWriteError::rejected(format!(
            "replace_text expected {expected_matches} matches but found {}.",
            ranges.len()
        )));
    }
    let mut result = content.to_owned();
    for range in ranges.iter().rev() {
        result.replace_range(range.clone(), new_text);
    }
    Ok((result, ranges.len()))
}

fn line_byte_range(
    content: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<(usize, usize), CodeWriteError> {
    let (Some(start_line), Some(end_line)) = (start_line, end_line) else {
        return Ok((0, content.len()));
    };
    if start_line == 0 || end_line < start_line {
        return Err(invalid_arguments("the line range is invalid"));
    }
    let mut starts = vec![0usize];
    starts.extend(content.match_indices('\n').map(|(index, _)| index + 1));
    let start = starts.get(start_line - 1).copied().unwrap_or(content.len());
    let end = starts.get(end_line).copied().unwrap_or(content.len());
    Ok((start, end))
}

fn unique_sibling(target: &Path, label: &str) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{label}-{}", Uuid::new_v4()))
}

fn session_result(session: &EditSession, reused: bool) -> Value {
    json!({
        "outcome": "already_applied",
        "changed": false,
        "changed_target_count": 0,
        "result": {
            "session_id": session.id,
            "reused": reused,
            "purpose": session.purpose,
        }
    })
}

fn path_summary(file: &FileState) -> Value {
    json!({
        "path": file.path,
        "change_kind": change_kind(file),
        "staged_operation_count": file.staged_operations,
    })
}

fn change_kind(file: &FileState) -> &'static str {
    if matches!(file.working, FileSnapshot::Missing) {
        "delete"
    } else if matches!(file.base, FileSnapshot::Missing) {
        "create"
    } else {
        "update"
    }
}

fn validate_write_size(content: &str) -> Result<(), CodeWriteError> {
    if content.len() <= MAXIMUM_WRITE_BYTES {
        Ok(())
    } else {
        Err(invalid_arguments("the staged file is too large"))
    }
}

fn required_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, CodeWriteError> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 4_096
                && value.trim() == *value
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| invalid_arguments(&format!("{field} is required")))?;
    Ok(value)
}

fn required_raw_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, CodeWriteError> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_arguments(&format!("{field} is required")))
}

fn optional_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<Option<String>, CodeWriteError> {
    let Some(value) = arguments.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| invalid_arguments(&format!("{field} is invalid")))?;
    if value.len() > maximum || value.chars().any(char::is_control) {
        return Err(invalid_arguments(&format!("{field} is invalid")));
    }
    Ok(Some(value.to_owned()))
}

fn optional_bool(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<bool>, CodeWriteError> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid_arguments(&format!("{field} is invalid")))
        })
        .transpose()
}

fn optional_usize(
    arguments: &Map<String, Value>,
    field: &str,
) -> Result<Option<usize>, CodeWriteError> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid_arguments(&format!("{field} is invalid")))
        })
        .transpose()
}

fn invalid_arguments(message: &str) -> CodeWriteError {
    CodeWriteError::rejected(format!("The project edit was rejected: {message}."))
}

fn stale_context(path: &str) -> CodeWriteError {
    CodeWriteError::rejected(format!(
        "The project edit context for {path} is stale; read the file again."
    ))
}

fn session_unavailable() -> CodeWriteError {
    CodeWriteError::rejected("The project edit session is unavailable.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(run_id: &str) -> EditScope {
        EditScope {
            owner_user_id: "owner-1".into(),
            workspace_scope_id: "workspace-1".into(),
            conversation_id: "conversation-1".into(),
            run_id: run_id.into(),
        }
    }

    fn object(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn stages_in_memory_and_commits_only_after_baseline_validation() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join("existing.txt"), "before\n").unwrap();
        let before = format!("{:x}", Sha256::digest(b"before\n"));
        let store = CodeWriteStore::default();
        let opened = store
            .call("open_edit_session", &Map::new(), scope("run-1"), root, &[])
            .unwrap();
        let session_id = opened["result"]["session_id"].as_str().unwrap();
        let staged = store
            .call(
                "stage_edit_batch",
                &object(json!({
                    "session_id": session_id,
                    "operations": [
                        {"kind": "replace_text", "path": "existing.txt", "old_text": "before", "new_text": "after", "expected_sha256": before},
                        {"kind": "write", "path": "nested/new.txt", "content": "new", "expected_sha256": null}
                    ]
                })),
                scope("run-1"),
                root,
                &[],
            )
            .unwrap();
        assert_eq!(staged["changed_target_count"], 2);
        assert_eq!(
            fs::read_to_string(root.join("existing.txt")).unwrap(),
            "before\n"
        );
        assert!(!root.join("nested/new.txt").exists());
        let committed = store
            .call(
                "commit_edit_session",
                &object(json!({"session_id": session_id})),
                scope("run-1"),
                root,
                &[],
            )
            .unwrap();
        assert_eq!(committed["changed_target_count"], 2);
        assert_eq!(
            fs::read_to_string(root.join("existing.txt")).unwrap(),
            "after\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("nested/new.txt")).unwrap(),
            "new"
        );
        assert!(!store.completion_verified("owner-1", "run-1"));
        store.record_project_read(&scope("run-1"));
        assert!(store.completion_verified("owner-1", "run-1"));
    }

    #[test]
    fn rejects_stale_baselines_and_preserves_the_session_for_abort() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join("file.txt"), "one").unwrap();
        let sha = format!("{:x}", Sha256::digest(b"one"));
        let store = CodeWriteStore::default();
        let opened = store
            .call("open_edit_session", &Map::new(), scope("run-1"), root, &[])
            .unwrap();
        let id = opened["result"]["session_id"].as_str().unwrap().to_owned();
        store
            .call(
                "stage_edit_batch",
                &object(json!({"session_id": id, "operations": [{
                    "kind": "write", "path": "file.txt", "content": "two", "expected_sha256": sha
                }]})),
                scope("run-1"),
                root,
                &[],
            )
            .unwrap();
        fs::write(root.join("file.txt"), "external").unwrap();
        let error = store
            .call(
                "commit_edit_session",
                &object(json!({"session_id": id})),
                scope("run-1"),
                root,
                &[],
            )
            .unwrap_err();
        assert!(error.known_pre_effect);
        store
            .call(
                "abort_edit_session",
                &object(json!({"session_id": id})),
                scope("run-1"),
                root,
                &[],
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(root.join("file.txt")).unwrap(),
            "external"
        );
    }

    #[test]
    fn binds_sessions_to_scope_root_and_owned_paths() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let store = CodeWriteStore::default();
        let owned = vec!["src".to_owned()];
        let opened = store
            .call(
                "open_edit_session",
                &Map::new(),
                scope("run-1"),
                root,
                &owned,
            )
            .unwrap();
        let id = opened["result"]["session_id"].as_str().unwrap();
        for (run, path) in [("run-2", "src/file.txt"), ("run-1", "README.md")] {
            assert!(store
                .call(
                    "stage_edit_batch",
                    &object(json!({"session_id": id, "operations": [{
                        "kind": "write", "path": path, "content": "x", "expected_sha256": null
                    }]})),
                    scope(run),
                    root,
                    &owned,
                )
                .is_err());
        }
        store.release_run("run-1");
        assert!(store
            .call(
                "abort_edit_session",
                &object(json!({"session_id": id})),
                scope("run-1"),
                root,
                &owned,
            )
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_and_unsafe_relative_paths() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let outside = directory.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        for path in ["../outside", "/absolute", "C:/drive", "escape/file.txt"] {
            assert!(resolve_path(&root, path).is_err(), "{path}");
        }
    }
}
