// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::diff::{build_diff, read_text_for_diff, DiffInput};
use super::edit::{apply_edit_text, EditRequest, EditMatchInfo};
use super::fs_ops::FsOps;
use super::outcome::{classify_file_modification_error, FileModificationOutcome};
use super::revision::ModificationRevisionGuard;
use super::service::{CodeMaintainerHooksRef, CodeMaintainerService, ToolContext};
use super::session::{
    EditSession, EditSessionStore, EntryKind, EntrySnapshot, SessionFileState,
};
use super::storage::ChangeLogStore;
use super::utils::{generate_id, sha256_bytes};

use crate::tool_registry::text_result;

type SharedSessionStore = Arc<Mutex<EditSessionStore>>;
type SharedRevisionGuard = Arc<Mutex<ModificationRevisionGuard>>;

pub(super) fn register_write_tools(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: SharedRevisionGuard,
    session_store: SharedSessionStore,
    _root: PathBuf,
    allow_writes: bool,
    max_file_bytes: i64,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    register_open_edit_session_tool(
        service,
        session_store.clone(),
        writes_note,
        workspace_note,
    );
    register_stage_edit_batch_tool(
        service,
        fs_ops.clone(),
        session_store.clone(),
        revision_guard.clone(),
        max_write_bytes,
        workspace_note,
    );
    register_commit_edit_session_tool(
        service,
        fs_ops,
        change_log,
        revision_guard,
        session_store.clone(),
        allow_writes,
        max_file_bytes,
        max_write_bytes,
        writes_note,
        workspace_note,
        hooks,
    );
    register_abort_edit_session_tool(service, session_store, workspace_note);
}

fn register_open_edit_session_tool(
    service: &mut CodeMaintainerService,
    session_store: SharedSessionStore,
    writes_note: &str,
    workspace_note: &str,
) {
    service.register_tool(
        "open_edit_session",
        &format!(
            "Open a write session for the current project workspace. Use one session, stage one or more edit batches against its in-memory snapshot, then finish with commit_edit_session or abort_edit_session.\n{}.\n{}",
            writes_note, workspace_note
        ),
        json!({
            "type": "object",
            "properties": {
                "purpose": { "type": "string" }
            },
            "additionalProperties": false
        }),
        Arc::new(move |_args, ctx| {
            let invocation = (|| {
                let handle = session_store
                    .lock()
                    .map_err(|_| "edit session store unavailable".to_string())?
                    .open_session(ctx.run_id, ctx.conversation_id);
                Ok(text_result(json!({
                    "outcome": FileModificationOutcome::AlreadyApplied,
                    "changed": false,
                    "changed_target_count": 0,
                    "result": handle.to_json(),
                    "message": "Edit session opened. Stage batches against this session before committing."
                })))
            })();
            record_file_modification_outcome("open_edit_session", ctx, &invocation);
            invocation
        }),
    );
}

fn register_stage_edit_batch_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    session_store: SharedSessionStore,
    revision_guard: SharedRevisionGuard,
    max_write_bytes: i64,
    workspace_note: &str,
) {
    service.register_tool(
        "stage_edit_batch",
        &format!(
            "Stage one or more ordered edit operations into an existing write session without touching the file system yet. Multiple operations may target the same file; they will be applied sequentially to the session snapshot. For the first operation that touches a path, expected_sha256 must match the latest successful read of the current file, or be null only when the path is confirmed absent (and for directory deletes).\n{}\nSupported operation kinds: write, replace_text, append, delete.",
            workspace_note
        ),
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "minLength": 1 },
                "operations": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": ["write", "replace_text", "append", "delete"]
                            },
                            "path": { "type": "string" },
                            "content": { "type": "string" },
                            "old_text": { "type": "string" },
                            "new_text": { "type": "string" },
                            "start_line": { "type": "integer", "minimum": 1 },
                            "end_line": { "type": "integer", "minimum": 1 },
                            "before_context": { "type": "string" },
                            "after_context": { "type": "string" },
                            "expected_matches": { "type": "integer", "minimum": 1 },
                            "expected_sha256": {
                                "type": ["string", "null"],
                                "pattern": "^[0-9a-f]{64}$"
                            }
                        },
                        "additionalProperties": false,
                        "required": ["kind", "path"]
                    }
                }
            },
            "additionalProperties": false,
            "required": ["session_id", "operations"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
                let session_id = required_string(&args, "session_id")?;
                let operations = args
                    .get("operations")
                    .and_then(Value::as_array)
                    .ok_or("operations is required".to_string())?;
                if operations.is_empty() {
                    return Err("operations must contain at least one item".to_string());
                }

                let mut store = session_store
                    .lock()
                    .map_err(|_| "edit session store unavailable".to_string())?;
                let session = store.get_mut(session_id, ctx.run_id)?;
                let mut batch_changed_paths = BTreeSet::new();
                let mut batch_matches: Vec<Value> = Vec::new();

                for operation in operations {
                    let outcome = apply_stage_operation(
                        session,
                        operation,
                        &fs_ops,
                        &revision_guard,
                        ctx,
                        max_write_bytes,
                    )?;
                    if outcome.changed {
                        batch_changed_paths.insert(outcome.path.clone());
                    }
                    if let Some(info) = outcome.match_info {
                        batch_matches.push(json!({
                            "path": outcome.path,
                            "match": info,
                        }));
                    }
                }

                session.staged_operation_count += operations.len();
                session.touch();
                let pending_paths = session.changed_paths();
                let pending_path_summaries = pending_paths
                    .iter()
                    .filter_map(|path| session.files.get(path))
                    .map(session_path_summary)
                    .collect::<Vec<_>>();
                let changed = !batch_changed_paths.is_empty();
                Ok(text_result(json!({
                    "outcome": FileModificationOutcome::from_changed(changed),
                    "changed": changed,
                    "changed_target_count": batch_changed_paths.len(),
                    "result": {
                        "session_id": session.id,
                        "staged_operation_count": session.staged_operation_count,
                        "batch_operation_count": operations.len(),
                        "batch_changed_paths": batch_changed_paths.into_iter().collect::<Vec<_>>(),
                        "pending_target_count": pending_paths.len(),
                        "pending_paths": pending_path_summaries,
                    },
                    "matches": batch_matches
                })))
            })();
            record_file_modification_outcome("stage_edit_batch", ctx, &invocation);
            invocation
        }),
    );
}

fn register_commit_edit_session_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: SharedRevisionGuard,
    session_store: SharedSessionStore,
    allow_writes: bool,
    max_file_bytes: i64,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    service.register_tool(
        "commit_edit_session",
        &format!(
            "Atomically commit the staged session snapshot to the current project workspace. The commit revalidates every touched path against the session baseline before making any change, so stale paths fail together instead of cascading. {}\n{}",
            writes_note, workspace_note
        ),
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "minLength": 1 }
            },
            "additionalProperties": false,
            "required": ["session_id"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
                if !allow_writes {
                    return Err("Writes are disabled.".to_string());
                }
                let session_id = required_string(&args, "session_id")?;
                let session = session_store
                    .lock()
                    .map_err(|_| "edit session store unavailable".to_string())?
                    .take(session_id, ctx.run_id)?;
                commit_session(
                    session,
                    &fs_ops,
                    &change_log,
                    &revision_guard,
                    ctx,
                    max_file_bytes,
                    max_write_bytes,
                    hooks.as_ref(),
                )
            })();
            record_file_modification_outcome("commit_edit_session", ctx, &invocation);
            invocation
        }),
    );
}

fn register_abort_edit_session_tool(
    service: &mut CodeMaintainerService,
    session_store: SharedSessionStore,
    workspace_note: &str,
) {
    service.register_tool(
        "abort_edit_session",
        &format!(
            "Abort a write session and discard its staged in-memory snapshot without touching the file system.\n{}",
            workspace_note
        ),
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "minLength": 1 }
            },
            "additionalProperties": false,
            "required": ["session_id"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
                let session_id = required_string(&args, "session_id")?;
                let session = session_store
                    .lock()
                    .map_err(|_| "edit session store unavailable".to_string())?
                    .take(session_id, ctx.run_id)?;
                Ok(text_result(json!({
                    "outcome": FileModificationOutcome::AlreadyApplied,
                    "changed": false,
                    "changed_target_count": 0,
                    "result": {
                        "session_id": session.id,
                        "discarded_target_count": session.changed_paths().len(),
                        "staged_operation_count": session.staged_operation_count,
                    },
                    "message": "Edit session aborted. All staged changes were discarded."
                })))
            })();
            record_file_modification_outcome("abort_edit_session", ctx, &invocation);
            invocation
        }),
    );
}

#[derive(Debug)]
struct StageOutcome {
    path: String,
    changed: bool,
    match_info: Option<EditMatchInfo>,
}

fn apply_stage_operation(
    session: &mut EditSession,
    operation: &Value,
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    max_write_bytes: i64,
) -> Result<StageOutcome, String> {
    let kind = required_string(operation, "kind")?;
    let path = required_string(operation, "path")?;
    validate_session_path_overlaps(session, path, kind)?;
    match kind {
        "write" => stage_write(
            session,
            operation,
            path,
            fs_ops,
            revision_guard,
            ctx,
            max_write_bytes,
        ),
        "replace_text" => stage_replace_text(
            session,
            operation,
            path,
            fs_ops,
            revision_guard,
            ctx,
            max_write_bytes,
        ),
        "append" => stage_append(
            session,
            operation,
            path,
            fs_ops,
            revision_guard,
            ctx,
            max_write_bytes,
        ),
        "delete" => stage_delete(session, operation, path, fs_ops, revision_guard, ctx),
        other => Err(format!("unsupported operation kind: {other}")),
    }
}

fn stage_write(
    session: &mut EditSession,
    operation: &Value,
    path: &str,
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    max_write_bytes: i64,
) -> Result<StageOutcome, String> {
    let content = required_string(operation, "content")?.to_string();
    enforce_write_size(&content, max_write_bytes)?;
    let expected = expected_sha256(operation, "expected_sha256")?;
    let state = get_or_load_session_file(session, path, expected, fs_ops, revision_guard, ctx)?;
    if state.working.kind == EntryKind::Directory {
        return Err("Target path is a directory.".to_string());
    }
    let changed = state.working.content.as_deref() != Some(content.as_str())
        || state.working.kind != EntryKind::File;
    state.working = EntrySnapshot::file(content.clone(), sha256_bytes(content.as_bytes()));
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: state.path.clone(),
        changed,
        match_info: None,
    })
}

fn stage_replace_text(
    session: &mut EditSession,
    operation: &Value,
    path: &str,
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    max_write_bytes: i64,
) -> Result<StageOutcome, String> {
    let old_text = required_string(operation, "old_text")?;
    let new_text = operation
        .get("new_text")
        .and_then(Value::as_str)
        .ok_or("new_text is required".to_string())?;
    let expected = expected_sha256(operation, "expected_sha256")?
        .ok_or("expected_sha256 must not be null for replace_text".to_string())?;
    let state = get_or_load_session_file(session, path, Some(expected), fs_ops, revision_guard, ctx)?;
    if state.working.kind != EntryKind::File {
        return Err("Target is not a file.".to_string());
    }
    let start_line = optional_usize(operation, "start_line");
    let end_line = optional_usize(operation, "end_line");
    let before_context = operation.get("before_context").and_then(Value::as_str);
    let after_context = operation.get("after_context").and_then(Value::as_str);
    let expected_matches = optional_usize(operation, "expected_matches");
    let current = state.working.content.clone().unwrap_or_default();
    let edit_result = apply_edit_text(
        current.as_str(),
        EditRequest {
            old_text,
            new_text,
            start_line,
            end_line,
            before_context,
            after_context,
            expected_matches,
        },
    )
    .map_err(|err| {
        let outcome = classify_file_modification_error(err.as_str());
        if matches!(
            outcome,
            FileModificationOutcome::StaleContext | FileModificationOutcome::ExpectedMatch
        ) {
            mark_failed_modification(revision_guard, ctx, path);
            edit_modification_error(
                outcome,
                err.as_str(),
                path,
                state.base.sha256.as_deref(),
                start_line,
                end_line,
                current.as_str(),
                old_text,
            )
        } else {
            err
        }
    })?;
    enforce_write_size(&edit_result.content, max_write_bytes)?;
    let changed = edit_result.changed;
    state.working = EntrySnapshot::file(
        edit_result.content.clone(),
        sha256_bytes(edit_result.content.as_bytes()),
    );
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: state.path.clone(),
        changed,
        match_info: Some(edit_result.info),
    })
}

fn stage_append(
    session: &mut EditSession,
    operation: &Value,
    path: &str,
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    max_write_bytes: i64,
) -> Result<StageOutcome, String> {
    let content = required_string(operation, "content")?;
    let expected = expected_sha256(operation, "expected_sha256")?;
    let state = get_or_load_session_file(session, path, expected, fs_ops, revision_guard, ctx)?;
    if state.working.kind == EntryKind::Directory {
        return Err("Target path is a directory.".to_string());
    }
    let mut next = state.working.content.clone().unwrap_or_default();
    next.push_str(content);
    enforce_write_size(&next, max_write_bytes)?;
    let changed = state.working.kind != EntryKind::File || next != state.working.content.clone().unwrap_or_default();
    state.working = EntrySnapshot::file(next.clone(), sha256_bytes(next.as_bytes()));
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: state.path.clone(),
        changed,
        match_info: None,
    })
}

fn stage_delete(
    session: &mut EditSession,
    operation: &Value,
    path: &str,
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
) -> Result<StageOutcome, String> {
    let expected = expected_sha256(operation, "expected_sha256")?;
    let state = get_or_load_session_file(session, path, expected, fs_ops, revision_guard, ctx)?;
    let changed = state.working.kind != EntryKind::Missing;
    state.working = EntrySnapshot::missing();
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: state.path.clone(),
        changed,
        match_info: None,
    })
}

fn get_or_load_session_file<'a>(
    session: &'a mut EditSession,
    path: &str,
    expected_sha256: Option<&str>,
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
) -> Result<&'a mut SessionFileState, String> {
    if !session.files.contains_key(path) {
        let snapshot = load_entry_snapshot(fs_ops, path)?;
        verify_session_baseline(
            revision_guard,
            ctx,
            path,
            expected_sha256,
            &snapshot,
            None,
            None,
        )?;
        session
            .files
            .insert(path.to_string(), SessionFileState::new(path, snapshot));
    } else if let Some(expected) = expected_sha256 {
        let state = session
            .files
            .get(path)
            .ok_or_else(|| format!("session path unexpectedly missing: {path}"))?;
        if state.base.kind != EntryKind::File || state.base.sha256.as_deref() != Some(expected) {
            return Err(format!(
                "expected_sha256 for {} does not match the active session baseline",
                path
            ));
        }
    }
    session
        .files
        .get_mut(path)
        .ok_or_else(|| format!("session path unexpectedly missing: {path}"))
}

fn commit_session(
    session: EditSession,
    fs_ops: &FsOps,
    change_log: &Arc<Mutex<ChangeLogStore>>,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    max_file_bytes: i64,
    max_write_bytes: i64,
    hooks: Option<&CodeMaintainerHooksRef>,
) -> Result<Value, String> {
    let changed_states = session
        .files
        .values()
        .filter(|state| state.has_change())
        .cloned()
        .collect::<Vec<_>>();
    if changed_states.is_empty() {
        return Ok(text_result(json!({
            "outcome": FileModificationOutcome::AlreadyApplied,
            "changed": false,
            "changed_target_count": 0,
            "result": {
                "session_id": session.id,
                "committed_paths": [],
                "staged_operation_count": session.staged_operation_count,
            },
            "message": "Session had no pending file-system changes. Nothing was committed."
        })));
    }

    let conflicts = changed_states
        .iter()
        .filter_map(|state| commit_conflict_for_state(fs_ops, revision_guard, ctx, state))
        .collect::<Vec<_>>();
    if !conflicts.is_empty() {
        return Err(commit_conflict_error(&conflicts));
    }

    for state in &changed_states {
        if let Some(content) = state.working.content.as_deref() {
            enforce_write_size(content, max_write_bytes)?;
        }
    }

    let mut applied = Vec::new();
    let mut rollback_applied = Vec::new();
    for state in &changed_states {
        let resolved = fs_ops.resolve_path(state.path.as_str())?;
        let before_diff = read_text_for_diff(&resolved, max_file_bytes).unwrap_or_else(DiffInput::omitted);
        let commit_result = apply_path_commit(state, resolved.as_path())?;
        rollback_applied.push(commit_result.rollback.clone());
        applied.push(CommittedPath {
            state: state.clone(),
            resolved_path: resolved,
            before_diff,
            result: commit_result,
        });
    }

    let mut failure: Option<String> = None;
    for committed in &applied {
        if let Err(error) = committed.result.finalize() {
            failure = Some(error);
            break;
        }
    }

    if let Some(error) = failure {
        rollback_commits(rollback_applied.into_iter().rev().collect());
        return Err(format!("commit_edit_session failed: {error}"));
    }

    let store = change_log
        .lock()
        .map_err(|_| "change log unavailable".to_string())?;
    let mut files = Vec::new();
    for committed in applied {
        let full_path = committed.resolved_path.to_string_lossy().to_string();
        let path = committed.state.path.clone();
        match committed.state.working.kind {
            EntryKind::Missing => {
                let diff = build_diff(committed.before_diff, DiffInput::text(String::new()));
                let record = store.log_change(
                    path.as_str(),
                    "commit_edit_session",
                    "delete",
                    0,
                    "",
                    ctx.conversation_id,
                    ctx.run_id,
                    diff,
                )?;
                note_workspace_path_changed(hooks, full_path.as_str());
                files.push(json!({
                    "path": path,
                    "change_kind": "delete",
                    "deleted": true,
                    "change": record,
                }));
            }
            EntryKind::File => {
                let content = committed.state.working.content.clone().unwrap_or_default();
                let sha256 = committed.state.working.sha256.clone().unwrap_or_default();
                let diff = build_diff(committed.before_diff, DiffInput::text(content.clone()));
                let change_kind = if committed.state.base.kind == EntryKind::Missing {
                    "create"
                } else {
                    "edit"
                };
                let bytes = i64::try_from(content.len()).map_err(|_| "write too large".to_string())?;
                let record = store.log_change(
                    path.as_str(),
                    "commit_edit_session",
                    change_kind,
                    bytes,
                    sha256.as_str(),
                    ctx.conversation_id,
                    ctx.run_id,
                    diff,
                )?;
                note_workspace_path_changed(hooks, full_path.as_str());
                files.push(json!({
                    "path": path,
                    "change_kind": change_kind,
                    "bytes": bytes,
                    "sha256": sha256,
                    "deleted": false,
                    "change": record,
                }));
            }
            EntryKind::Directory => {}
        }
    }

    Ok(text_result(json!({
        "outcome": FileModificationOutcome::Changed,
        "changed": true,
        "changed_target_count": files.len(),
        "result": {
            "session_id": session.id,
            "staged_operation_count": session.staged_operation_count,
            "committed_paths": files,
            "session_closed": true,
        }
    })))
}

#[derive(Debug)]
struct CommittedPath {
    state: SessionFileState,
    resolved_path: PathBuf,
    before_diff: DiffInput,
    result: PathCommitResult,
}

#[derive(Debug)]
struct PathCommitResult {
    rollback: RollbackAction,
}

impl PathCommitResult {
    fn finalize(&self) -> Result<(), String> {
        self.rollback.cleanup()
    }
}

#[derive(Debug, Clone)]
enum RollbackAction {
    None,
    CreatedFile { path: PathBuf },
    ReplacedFile {
        path: PathBuf,
        backup: PathBuf,
        created: PathBuf,
    },
    DeletedEntry {
        path: PathBuf,
        backup: PathBuf,
    },
}

impl RollbackAction {
    fn rollback(self) {
        match self {
            Self::None => {}
            Self::CreatedFile { path } => {
                let _ = fs::remove_file(path);
            }
            Self::ReplacedFile {
                path,
                backup,
                created,
            } => {
                let _ = fs::remove_file(path.as_path());
                let _ = fs::rename(backup.as_path(), path.as_path());
                let _ = fs::remove_file(created.as_path());
            }
            Self::DeletedEntry { path, backup } => {
                let _ = fs::rename(backup.as_path(), path.as_path());
            }
        }
    }

    fn cleanup(&self) -> Result<(), String> {
        match self {
            Self::None | Self::CreatedFile { .. } => Ok(()),
            Self::ReplacedFile { backup, created, .. } => {
                if backup.exists() {
                    fs::remove_file(backup).map_err(|err| err.to_string())?;
                }
                if created.exists() {
                    fs::remove_file(created).map_err(|err| err.to_string())?;
                }
                Ok(())
            }
            Self::DeletedEntry { backup, .. } => {
                if backup.exists() {
                    if backup.is_dir() {
                        fs::remove_dir_all(backup).map_err(|err| err.to_string())?;
                    } else {
                        fs::remove_file(backup).map_err(|err| err.to_string())?;
                    }
                }
                Ok(())
            }
        }
    }
}

fn apply_path_commit(state: &SessionFileState, resolved: &Path) -> Result<PathCommitResult, String> {
    match state.working.kind {
        EntryKind::Missing => apply_delete_commit(resolved),
        EntryKind::File => apply_file_commit(state, resolved),
        EntryKind::Directory => Err("directory staging is not commit-compatible".to_string()),
    }
}

fn apply_delete_commit(resolved: &Path) -> Result<PathCommitResult, String> {
    if !resolved.exists() {
        return Ok(PathCommitResult {
            rollback: RollbackAction::None,
        });
    }
    let backup = sibling_temp_path(resolved, "delete_backup");
    fs::rename(resolved, backup.as_path()).map_err(|err| err.to_string())?;
    Ok(PathCommitResult {
        rollback: RollbackAction::DeletedEntry {
            path: resolved.to_path_buf(),
            backup,
        },
    })
}

fn apply_file_commit(state: &SessionFileState, resolved: &Path) -> Result<PathCommitResult, String> {
    let content = state.working.content.clone().unwrap_or_default();
    let created = sibling_temp_path(resolved, "write_stage");
    if let Some(parent) = created.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(created.as_path(), content.as_bytes()).map_err(|err| err.to_string())?;
    if !resolved.exists() {
        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::rename(created.as_path(), resolved).map_err(|err| err.to_string())?;
        return Ok(PathCommitResult {
            rollback: RollbackAction::CreatedFile {
                path: resolved.to_path_buf(),
            },
        });
    }
    let backup = sibling_temp_path(resolved, "write_backup");
    fs::rename(resolved, backup.as_path()).map_err(|err| err.to_string())?;
    fs::rename(created.as_path(), resolved).map_err(|err| err.to_string())?;
    Ok(PathCommitResult {
        rollback: RollbackAction::ReplacedFile {
            path: resolved.to_path_buf(),
            backup,
            created,
        },
    })
}

fn rollback_commits(actions: Vec<RollbackAction>) {
    for action in actions {
        action.rollback();
    }
}

fn commit_conflict_for_state(
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    state: &SessionFileState,
) -> Option<Value> {
    let current = load_entry_snapshot(fs_ops, state.path.as_str()).ok()?;
    if state.base.kind == current.kind {
        if state.base.kind != EntryKind::File || state.base.sha256 == current.sha256 {
            return None;
        }
    }
    if matches!(state.base.kind, EntryKind::File) || matches!(current.kind, EntryKind::File) {
        mark_failed_modification(revision_guard, ctx, state.path.as_str());
    }
    let recovery_tool = if matches!(state.base.kind, EntryKind::File) || matches!(current.kind, EntryKind::File) {
        "read_file_raw"
    } else {
        "list_dir"
    };
    Some(json!({
        "path": state.path,
        "baseline_sha256": state.base.sha256,
        "latest_sha256": current.sha256,
        "baseline_kind": entry_kind_name(&state.base.kind),
        "latest_kind": entry_kind_name(&current.kind),
        "recovery": {
            "required_next_tool": recovery_tool,
            "recommended_args": recovery_args(state.path.as_str(), recovery_tool),
        }
    }))
}

fn validate_session_path_overlaps(
    session: &EditSession,
    path: &str,
    kind: &str,
) -> Result<(), String> {
    let normalized = normalize_path(path);
    for existing in session.files.values() {
        if existing.path == normalized {
            continue;
        }
        let nested = normalized.starts_with(format!("{}/", existing.path).as_str())
            || existing.path.starts_with(format!("{}/", normalized).as_str());
        if nested
            && (kind == "delete"
                || (existing.base.kind == EntryKind::Directory
                    && existing.working.kind == EntryKind::Missing))
        {
            return Err(format!(
                "staged path {} conflicts with overlapping session path {}",
                normalized, existing.path
            ));
        }
    }
    Ok(())
}

fn load_entry_snapshot(fs_ops: &FsOps, path: &str) -> Result<EntrySnapshot, String> {
    let resolved = fs_ops.resolve_path(path)?;
    if !resolved.exists() {
        return Ok(EntrySnapshot::missing());
    }
    let metadata = fs::symlink_metadata(&resolved).map_err(|err| err.to_string())?;
    if metadata.is_dir() {
        return Ok(EntrySnapshot::directory());
    }
    if !metadata.is_file() && !metadata.file_type().is_symlink() {
        return Err("Target path is not a regular file or directory.".to_string());
    }
    let (_, _, sha256, content) = fs_ops.read_file_raw(path)?;
    Ok(EntrySnapshot::file(content, sha256))
}

fn enforce_write_size(content: &str, max_write_bytes: i64) -> Result<(), String> {
    if content.as_bytes().len() as i64 > max_write_bytes {
        return Err("Write exceeds max-write-bytes limit.".to_string());
    }
    Ok(())
}

fn verify_session_baseline(
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    path: &str,
    expected: Option<&str>,
    current: &EntrySnapshot,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<(), String> {
    let mut guard = revision_guard
        .lock()
        .map_err(|_| "file revision guard unavailable".to_string())?;
    if guard.is_reread_required(ctx.run_id, path) {
        let tool = if matches!(current.kind, EntryKind::File) {
            "read_file_raw"
        } else {
            "list_dir"
        };
        return Err(file_revision_error(
            "stale_context",
            "A successful workspace read is required after the previous failed modification",
            path,
            current.sha256.as_deref(),
            start_line,
            end_line,
            tool,
        ));
    }
    match current.kind {
        EntryKind::File => {
            if expected == current.sha256.as_deref() {
                return Ok(());
            }
            guard.require_reread(ctx.run_id, path);
            Err(file_revision_error(
                "stale_context",
                "The target file revision does not match the staged request",
                path,
                current.sha256.as_deref(),
                start_line,
                end_line,
                "read_file_raw",
            ))
        }
        EntryKind::Missing => {
            if expected.is_none() {
                return Ok(());
            }
            Err(file_revision_error(
                "stale_context",
                "The target path no longer exists at the requested revision",
                path,
                None,
                start_line,
                end_line,
                "list_dir",
            ))
        }
        EntryKind::Directory => {
            if expected.is_none() {
                return Ok(());
            }
            Err(file_revision_error(
                "stale_context",
                "The target path is now a directory instead of the requested file revision",
                path,
                None,
                start_line,
                end_line,
                "list_dir",
            ))
        }
    }
}

fn mark_failed_modification(
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
    path: &str,
) {
    if let Ok(mut guard) = revision_guard.lock() {
        guard.require_reread(ctx.run_id, path);
    }
}

fn file_revision_error(
    category: &str,
    message: &str,
    path: &str,
    latest_sha256: Option<&str>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    recovery_tool: &str,
) -> String {
    serde_json::to_string(&json!({
        "category": category,
        "error": message,
        "path": path,
        "latest_sha256": latest_sha256,
        "conflict_range": {
            "start_line": start_line,
            "end_line": end_line,
        },
        "recovery": {
            "required_next_tool": recovery_tool,
            "recommended_args": recovery_args(path, recovery_tool),
            "guidance": "Read the current workspace state again, open a fresh edit session, then rebuild the staged batch from the latest content."
        }
    }))
    .unwrap_or_else(|_| format!("{category}: {message}"))
}

fn recovery_args(path: &str, recovery_tool: &str) -> Value {
    match recovery_tool {
        "list_dir" => json!({ "path": parent_or_dot(path) }),
        _ => json!({ "path": path }),
    }
}

fn commit_conflict_error(conflicts: &[Value]) -> String {
    let first = conflicts.first().cloned().unwrap_or_else(|| json!({}));
    serde_json::to_string(&json!({
        "category": "stale_context",
        "error": "One or more staged paths changed after the session was opened. The session was closed without applying any file-system changes.",
        "path": first.get("path").cloned().unwrap_or(Value::Null),
        "latest_sha256": first.get("latest_sha256").cloned().unwrap_or(Value::Null),
        "conflicts": conflicts,
        "recovery": {
            "required_next_tool": "read_file_raw",
            "guidance": "Re-read every conflicted path, open a new edit session, and restage the batch against the latest content."
        }
    }))
    .unwrap_or_else(|_| "stale_context: staged session conflict".to_string())
}

fn edit_modification_error(
    outcome: FileModificationOutcome,
    message: &str,
    path: &str,
    latest_sha256: Option<&str>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    content: &str,
    old_text: &str,
) -> String {
    let base = file_revision_error(
        outcome.as_str(),
        message,
        path,
        latest_sha256,
        start_line,
        end_line,
        "read_file_raw",
    );
    let Ok(mut payload) = serde_json::from_str::<Value>(base.as_str()) else {
        return base;
    };
    payload["candidate_summary"] = edit_candidate_summary(content, old_text);
    serde_json::to_string(&payload).unwrap_or(base)
}

fn edit_candidate_summary(content: &str, old_text: &str) -> Value {
    if old_text.is_empty() {
        return json!({ "count": 0, "candidates": [] });
    }
    let lines = content.split('\n').collect::<Vec<_>>();
    let mut candidates = Vec::new();
    let mut offset = 0usize;
    let mut count = 0usize;
    while let Some(relative) = content[offset..].find(old_text) {
        let start = offset + relative;
        let line = content[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        count += 1;
        if candidates.len() < 8 {
            let first_line = line.saturating_sub(1).max(1);
            let last_line = (line + 1).min(lines.len().max(1));
            let context = lines
                .iter()
                .enumerate()
                .skip(first_line - 1)
                .take(last_line - first_line + 1)
                .map(|(index, text)| format!("{}: {}", index + 1, text))
                .collect::<Vec<_>>()
                .join("\n");
            candidates.push(json!({
                "ordinal": count,
                "line": line,
                "context": context.chars().take(600).collect::<String>(),
            }));
        }
        offset = start + old_text.len();
    }
    json!({
        "count": count,
        "truncated": count > candidates.len(),
        "candidates": candidates,
    })
}

fn sibling_temp_path(path: &Path, label: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace");
    let temp_name = format!(".{}.{}.tmp", file_name, generate_id(label));
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(temp_name)
}

fn session_path_summary(state: &SessionFileState) -> Value {
    json!({
        "path": state.path,
        "baseline_kind": entry_kind_name(&state.base.kind),
        "staged_kind": entry_kind_name(&state.working.kind),
        "changed": state.has_change(),
        "staged_operations": state.staged_operations,
        "staged_sha256": state.working_sha256(),
    })
}

fn entry_kind_name(kind: &EntryKind) -> &'static str {
    match kind {
        EntryKind::Missing => "missing",
        EntryKind::File => "file",
        EntryKind::Directory => "directory",
    }
}

fn note_workspace_path_changed(hooks: Option<&CodeMaintainerHooksRef>, path: &str) {
    if let Some(hooks) = hooks {
        hooks.note_workspace_path_changed(path);
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn parent_or_dot(path: &str) -> String {
    let normalized = normalize_path(path);
    Path::new(normalized.as_str())
        .parent()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(".")
        .to_string()
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} is required"))
}

fn optional_usize(value: &Value, field: &str) -> Option<usize> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

fn expected_sha256<'a>(args: &'a Value, field: &str) -> Result<Option<&'a str>, String> {
    match args.get(field) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if is_sha256(value) => Ok(Some(value.as_str())),
        Some(Value::String(_)) => Err(format!(
            "{field} must be a lowercase 64-character SHA-256 value"
        )),
        Some(_) => Err(format!("{field} must be a SHA-256 string or null")),
        None => Err(format!("{field} is required")),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn record_file_modification_outcome(
    tool: &str,
    ctx: &ToolContext<'_>,
    invocation: &Result<Value, String>,
) {
    let (outcome, success, changed, changed_target_count) = match invocation {
        Ok(value) => {
            let payload = value.get("_structured_result").unwrap_or(value);
            let changed = payload
                .get("changed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let outcome = payload
                .get("outcome")
                .and_then(Value::as_str)
                .unwrap_or_else(|| FileModificationOutcome::from_changed(changed).as_str());
            let changed_target_count = payload
                .get("changed_target_count")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(changed));
            (outcome, true, changed, changed_target_count)
        }
        Err(error) => {
            let outcome = classify_file_modification_error(error);
            (outcome.as_str(), outcome.is_success(), false, 0)
        }
    };
    tracing::info!(
        event = "file_modification_outcome",
        source = "builtin_code_maintainer",
        tool,
        conversation_id = ctx.conversation_id,
        run_id = ctx.run_id,
        outcome,
        success,
        changed,
        changed_target_count,
        "file modification completed"
    );
}
