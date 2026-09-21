// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::diff::{build_diff, read_text_for_diff, DiffInput};
use super::edit::{apply_edit_text, EditMatchInfo, EditRequest};
use super::fs_ops::FsOps;
use super::outcome::{classify_file_modification_error, FileModificationOutcome};
use super::revision::ModificationRevisionGuard;
use super::service::{CodeMaintainerHooksRef, CodeMaintainerService, ToolContext};
use super::session::{EditSession, EditSessionStore, EntryKind, EntrySnapshot, SessionFileState};
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
    register_open_edit_session_tool(service, session_store.clone(), writes_note, workspace_note);
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
            "Open a write session for the current project workspace. Use one session, stage one or more edit batches against its in-memory snapshot, then finish with commit_edit_session or abort_edit_session. Set fresh=true after a stale-context or expected-match recovery when the previous session must be discarded and rebased from the latest workspace state.\n{}.\n{}",
            writes_note, workspace_note
        ),
        json!({
            "type": "object",
            "properties": {
                "purpose": { "type": "string" },
                "fresh": {
                    "type": "boolean",
                    "description": "Discard the reusable session for this run/conversation and create a new baseline from the current workspace."
                }
            },
            "additionalProperties": false
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
                let fresh = args
                    .get("fresh")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let handle = session_store
                    .lock()
                    .map_err(|_| "edit session store unavailable".to_string())?
                    .open_session(ctx.run_id, ctx.conversation_id, fresh);
                let message = if handle.reused {
                    "An active edit session already exists for this run. Reuse the returned session_id; do not open another session. Stage any remaining batches, then call commit_edit_session or abort_edit_session."
                } else if fresh {
                    "Fresh edit session opened from the current workspace baseline. Rebuild and stage the batch against this session before committing."
                } else {
                    "Edit session opened. Stage batches against this session before committing."
                };
                Ok(text_result(json!({
                    "outcome": FileModificationOutcome::AlreadyApplied,
                    "changed": false,
                    "changed_target_count": 0,
                    "result": handle.to_json(),
                    "message": message
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
                            "start_line": {
                                "type": "integer",
                                "minimum": 1,
                                "description": "Optional inclusive lower bound for the first line where old_text may start."
                            },
                            "end_line": {
                                "type": "integer",
                                "minimum": 1,
                                "description": "Optional inclusive upper bound for the first line where old_text may start; multi-line old_text may extend beyond it."
                            },
                            "before_context": {
                                "type": "string",
                                "description": "Optional exact anchor expected immediately before old_text or within the preceding 12 lines."
                            },
                            "after_context": {
                                "type": "string",
                                "description": "Optional exact anchor expected immediately after old_text or within the following 12 lines."
                            },
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
                let session = store.get_mut(session_id, ctx.run_id, ctx.conversation_id)?;
                let mut staged_session = session.clone();
                let mut batch_changed_paths = BTreeSet::new();
                let mut batch_matches: Vec<Value> = Vec::new();

                for operation in operations {
                    let outcome = apply_stage_operation(
                        &mut staged_session,
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

                staged_session.staged_operation_count += operations.len();
                staged_session.touch();
                let pending_paths = staged_session.changed_paths();
                let pending_path_summaries = pending_paths
                    .iter()
                    .filter_map(|path| staged_session.files.get(path))
                    .map(session_path_summary)
                    .collect::<Vec<_>>();
                let changed = !batch_changed_paths.is_empty();
                *session = staged_session;
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
                    .take(session_id, ctx.run_id, ctx.conversation_id)?;
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
                    .take(session_id, ctx.run_id, ctx.conversation_id)?;
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
    fs_ops.resolve_write_path(path)?;
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
    if let Some(state) = session.files.get_mut(path) {
        if state.working.kind == EntryKind::File
            && state.working.content.as_deref() == Some(content.as_str())
        {
            state.staged_operations += 1;
            return Ok(StageOutcome {
                path: state.path.clone(),
                changed: false,
                match_info: None,
            });
        }
    } else {
        let snapshot = load_entry_snapshot(fs_ops, path)?;
        if snapshot.kind == EntryKind::File && snapshot.content.as_deref() == Some(content.as_str())
        {
            let mut state = SessionFileState::new(path, snapshot);
            state.staged_operations += 1;
            session.files.insert(path.to_string(), state);
            return Ok(StageOutcome {
                path: path.to_string(),
                changed: false,
                match_info: None,
            });
        }
    }
    let expected = expected_revision(operation, "expected_sha256")?;
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
    let start_line = optional_usize(operation, "start_line");
    let end_line = optional_usize(operation, "end_line");
    let before_context = operation.get("before_context").and_then(Value::as_str);
    let after_context = operation.get("after_context").and_then(Value::as_str);
    let expected_matches = optional_usize(operation, "expected_matches");
    let request = EditRequest {
        old_text,
        new_text,
        start_line,
        end_line,
        before_context,
        after_context,
        expected_matches,
    };

    if let Some(state) = session.files.get_mut(path) {
        if state.working.kind == EntryKind::File {
            let current = state.working.content.clone().unwrap_or_default();
            if let Ok(edit_result) = apply_edit_text(current.as_str(), request) {
                if !edit_result.changed {
                    state.staged_operations += 1;
                    return Ok(StageOutcome {
                        path: state.path.clone(),
                        changed: false,
                        match_info: Some(edit_result.info),
                    });
                }
            }
        }
    } else {
        let snapshot = load_entry_snapshot(fs_ops, path)?;
        if snapshot.kind == EntryKind::File {
            let current = snapshot.content.clone().unwrap_or_default();
            if let Ok(edit_result) = apply_edit_text(current.as_str(), request) {
                if !edit_result.changed {
                    let mut state = SessionFileState::new(path, snapshot);
                    state.staged_operations += 1;
                    session.files.insert(path.to_string(), state);
                    return Ok(StageOutcome {
                        path: path.to_string(),
                        changed: false,
                        match_info: Some(edit_result.info),
                    });
                }
            }
        }
    }

    let expected = expected_revision(operation, "expected_sha256")?;
    let state = get_or_load_session_file(session, path, expected, fs_ops, revision_guard, ctx)?;
    if state.working.kind != EntryKind::File {
        return Err("Target is not a file.".to_string());
    }
    let current = state.working.content.clone().unwrap_or_default();
    let edit_result = apply_edit_text(current.as_str(), request).map_err(|err| {
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
    let expected = expected_revision(operation, "expected_sha256")?;
    let state = get_or_load_session_file(session, path, expected, fs_ops, revision_guard, ctx)?;
    if state.working.kind == EntryKind::Directory {
        return Err("Target path is a directory.".to_string());
    }
    let mut next = state.working.content.clone().unwrap_or_default();
    next.push_str(content);
    enforce_write_size(&next, max_write_bytes)?;
    let changed = state.working.kind != EntryKind::File
        || next != state.working.content.clone().unwrap_or_default();
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
    let expected = expected_revision(operation, "expected_sha256")?;
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
    expected_revision: ExpectedRevision<'_>,
    fs_ops: &FsOps,
    revision_guard: &SharedRevisionGuard,
    ctx: &ToolContext<'_>,
) -> Result<&'a mut SessionFileState, String> {
    if !session.files.contains_key(path) {
        let expected_sha256 = expected_revision
            .into_value()
            .ok_or_else(|| "expected_sha256 is required when a path is first staged".to_string())?;
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
    } else if let Some(expected_sha256) = expected_revision.into_value() {
        let state = session
            .files
            .get(path)
            .ok_or_else(|| format!("session path unexpectedly missing: {path}"))?;
        let matches = snapshot_matches_expected(&state.base, expected_sha256)
            || snapshot_matches_expected(&state.working, expected_sha256);
        if !matches {
            let latest = load_entry_snapshot(fs_ops, path)?;
            mark_failed_modification(revision_guard, ctx, path);
            return Err(session_baseline_mismatch_error(
                path,
                latest.sha256.as_deref(),
            ));
        }
    }
    session
        .files
        .get_mut(path)
        .ok_or_else(|| format!("session path unexpectedly missing: {path}"))
}

fn snapshot_matches_expected(snapshot: &EntrySnapshot, expected_sha256: Option<&str>) -> bool {
    match snapshot.kind {
        EntryKind::File => snapshot.sha256.as_deref() == expected_sha256,
        EntryKind::Missing | EntryKind::Directory => expected_sha256.is_none(),
    }
}

include!("registration_write_part01.rs");
include!("registration_write_part02.rs");
