// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::diff::{
    build_diff, extract_patch_diffs, extract_patch_targets, read_text_for_diff, DiffInput,
};
use super::edit::{apply_edit_text, EditRequest};
use super::fs_ops::FsOps;
use super::outcome::{classify_file_modification_error, FileModificationOutcome};
use super::patch::apply_patch_limited;
use super::revision::ModificationRevisionGuard;
use super::service::{CodeMaintainerHooksRef, CodeMaintainerService};
use super::storage::ChangeLogStore;
use super::utils::format_bytes;
use sha2::{Digest, Sha256};

use crate::tool_registry::text_result;

pub(super) fn register_write_tools(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: Arc<Mutex<ModificationRevisionGuard>>,
    root: PathBuf,
    allow_writes: bool,
    max_file_bytes: i64,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    register_write_file_tool(
        service,
        fs_ops.clone(),
        change_log.clone(),
        revision_guard.clone(),
        max_write_bytes,
        writes_note,
        workspace_note,
        hooks.clone(),
    );
    register_edit_file_tool(
        service,
        fs_ops.clone(),
        change_log.clone(),
        revision_guard.clone(),
        workspace_note,
        hooks.clone(),
    );
    register_append_file_tool(
        service,
        fs_ops.clone(),
        change_log.clone(),
        revision_guard.clone(),
        max_write_bytes,
        writes_note,
        workspace_note,
        hooks.clone(),
    );
    register_delete_path_tool(
        service,
        fs_ops.clone(),
        change_log.clone(),
        revision_guard.clone(),
        max_file_bytes,
        writes_note,
        workspace_note,
        hooks.clone(),
    );
    register_apply_patch_tool(
        service,
        fs_ops,
        change_log,
        revision_guard,
        root,
        allow_writes,
        max_file_bytes,
        max_write_bytes,
        writes_note,
        workspace_note,
        hooks,
    );
}

fn register_write_file_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: Arc<Mutex<ModificationRevisionGuard>>,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    service.register_tool(
        "write_file",
        &format!(
            "Write file content to the current project workspace. Use this for new files or full-file replacement when the target path is known. expected_sha256 is mandatory: use the sha256 returned by the latest read for an existing file, or null only when creating a path confirmed absent.\nMax write bytes: {}.\n{}.\n{workspace_note}",
            format_bytes(max_write_bytes),
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "expected_sha256": {
                    "type": ["string", "null"],
                    "pattern": "^[0-9a-f]{64}$"
                }
            },
            "additionalProperties": false,
            "required": ["path", "content", "expected_sha256"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let content = args
                .get("content")
                .and_then(|value| value.as_str())
                .ok_or("content is required".to_string())?;
            let expected_sha256 = expected_sha256(&args, "expected_sha256")?;
            let target = fs_ops.resolve_path(path)?;
            let existed_before = target.exists();
            let (current_sha256, before_snapshot) = if existed_before {
                let (_, _, sha256, current_content) = fs_ops.read_file_raw(path)?;
                (Some(sha256), DiffInput::text(current_content))
            } else {
                (None, DiffInput { text: None, reason: None })
            };
            verify_file_revision(
                &revision_guard,
                ctx,
                path,
                expected_sha256,
                current_sha256.as_deref(),
                None,
                None,
            )?;
            if before_snapshot.text.as_deref() == Some(content) {
                return Ok(text_result(json!({
                    "outcome": FileModificationOutcome::AlreadyApplied,
                    "changed": false,
                    "changed_target_count": 0,
                    "result": {
                        "path": path,
                        "bytes": content.len() as i64,
                        "sha256": current_sha256,
                        "changed": false,
                        "already_applied": true
                    },
                    "message": "The requested full-file content is already present. No file-system change was applied."
                })));
            }
            let result = fs_ops.write_file(path, content)?;
            let after_snapshot = DiffInput::text(content.to_string());
            let diff = build_diff(before_snapshot, after_snapshot);
            let full_path = target.to_string_lossy().to_string();
            let record = change_log
                .lock()
                .map_err(|_| "change log unavailable".to_string())?
                .log_change(
                    &result.path,
                    "write",
                    if existed_before { "edit" } else { "create" },
                    result.bytes,
                    &result.sha256,
                    ctx.conversation_id,
                    ctx.run_id,
                    diff,
                )?;
            note_workspace_path_changed(hooks.as_ref(), full_path.as_str());
            Ok(text_result(json!({
                "outcome": FileModificationOutcome::Changed,
                "changed": true,
                "changed_target_count": 1,
                "previous_sha256": current_sha256,
                "result": result,
                "change": record
            })))
            })();
            record_file_modification_outcome("write_file", ctx, &invocation);
            invocation
        }),
    );
}

fn register_edit_file_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: Arc<Mutex<ModificationRevisionGuard>>,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    service.register_tool(
        "edit_file",
        &format!(
            "Safely edit a file in the current project workspace by replacing old_text with new_text. expected_sha256 must be the sha256 returned by the latest successful read of this file. After a stale_context or expected_match failure, re-read the target before retrying.\nWhen old_text appears multiple times, you MUST provide more surrounding context (before_context / after_context, recommended 1-3 lines) or narrow start_line/end_line. Context may be supplied as adjacent whole lines without manually adding the boundary newline.\n{workspace_note}"
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old_text": { "type": "string", "minLength": 1 },
                "new_text": { "type": "string" },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "before_context": { "type": "string" },
                "after_context": { "type": "string" },
                "expected_matches": { "type": "integer", "minimum": 1 },
                "expected_sha256": {
                    "type": "string",
                    "pattern": "^[0-9a-f]{64}$"
                }
            },
            "additionalProperties": false,
            "required": ["path", "old_text", "new_text", "expected_sha256"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let old_text = args
                .get("old_text")
                .and_then(|value| value.as_str())
                .ok_or("old_text is required".to_string())?;
            let new_text = args
                .get("new_text")
                .and_then(|value| value.as_str())
                .ok_or("new_text is required".to_string())?;
            let start_line = args
                .get("start_line")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize);
            let end_line = args
                .get("end_line")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize);
            let before_context = args.get("before_context").and_then(|value| value.as_str());
            let after_context = args.get("after_context").and_then(|value| value.as_str());
            let expected_matches = args
                .get("expected_matches")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize);
            let expected_sha256 = expected_sha256(&args, "expected_sha256")?
                .ok_or("expected_sha256 must not be null for edit_file".to_string())?;

            let (resolved_path, size, sha, content) = fs_ops.read_file_raw(path)?;
            verify_file_revision(
                &revision_guard,
                ctx,
                path,
                Some(expected_sha256),
                Some(sha.as_str()),
                start_line,
                end_line,
            )?;
            let edit_result = apply_edit_text(
                &content,
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
                    FileModificationOutcome::StaleContext
                        | FileModificationOutcome::ExpectedMatch
                ) {
                    mark_failed_modification(&revision_guard, ctx, path);
                    edit_modification_error(
                        outcome,
                        err.as_str(),
                        path,
                        Some(sha.as_str()),
                        start_line,
                        end_line,
                        content.as_str(),
                        old_text,
                    )
                } else {
                    err
                }
            })?;

            if !edit_result.changed {
                return Ok(text_result(json!({
                    "outcome": FileModificationOutcome::AlreadyApplied,
                    "changed": false,
                    "changed_target_count": 0,
                    "result": {
                        "path": resolved_path,
                        "bytes": size,
                        "sha256": sha,
                        "changed": false,
                        "already_applied": true
                    },
                    "match": edit_result.info,
                    "message": "The requested edit is already present. No file-system change was applied."
                })));
            }

            let updated_content = edit_result.content.clone();
            let write_result = fs_ops.write_file(path, &updated_content)?;
            let diff = build_diff(DiffInput::text(content), DiffInput::text(updated_content));
            let full_path = fs_ops.resolve_path(path)?.to_string_lossy().to_string();
            let record = change_log
                .lock()
                .map_err(|_| "change log unavailable".to_string())?
                .log_change(
                    &write_result.path,
                    "edit_file",
                    "edit",
                    write_result.bytes,
                    &write_result.sha256,
                    ctx.conversation_id,
                    ctx.run_id,
                    diff,
                )?;
            note_workspace_path_changed(hooks.as_ref(), full_path.as_str());
            Ok(text_result(json!({
                "outcome": FileModificationOutcome::Changed,
                "changed": true,
                "changed_target_count": 1,
                "result": write_result,
                "match": edit_result.info,
                "change": record
            })))
            })();
            record_file_modification_outcome("edit_file", ctx, &invocation);
            invocation
        }),
    );
}

fn register_append_file_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: Arc<Mutex<ModificationRevisionGuard>>,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    service.register_tool(
        "append_file",
        &format!(
            "Append content to a file in the current project workspace. expected_sha256 is mandatory: use the sha256 returned by the latest read for an existing file, or null only when creating a path confirmed absent.\nMax write bytes: {}.\n{}.\n{workspace_note}",
            format_bytes(max_write_bytes),
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "expected_sha256": {
                    "type": ["string", "null"],
                    "pattern": "^[0-9a-f]{64}$"
                }
            },
            "additionalProperties": false,
            "required": ["path", "content", "expected_sha256"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let content = args
                .get("content")
                .and_then(|value| value.as_str())
                .ok_or("content is required".to_string())?;
            let expected_sha256 = expected_sha256(&args, "expected_sha256")?;
            let target = fs_ops.resolve_path(path)?;
            let existed_before = target.exists();
            let (current_sha256, before_snapshot) = if existed_before {
                let (_, _, sha256, current_content) = fs_ops.read_file_raw(path)?;
                (Some(sha256), DiffInput::text(current_content))
            } else {
                (None, DiffInput { text: None, reason: None })
            };
            verify_file_revision(
                &revision_guard,
                ctx,
                path,
                expected_sha256,
                current_sha256.as_deref(),
                None,
                None,
            )?;
            if existed_before && content.is_empty() {
                return Ok(text_result(json!({
                    "outcome": FileModificationOutcome::AlreadyApplied,
                    "changed": false,
                    "changed_target_count": 0,
                    "result": {
                        "path": path,
                        "sha256": current_sha256,
                        "changed": false,
                        "already_applied": true
                    },
                    "message": "The append content is empty. No file-system change was applied."
                })));
            }
            let after_snapshot = if let Some(reason) = before_snapshot.reason.clone() {
                DiffInput::omitted(reason)
            } else {
                let mut next = before_snapshot.text.clone().unwrap_or_default();
                next.push_str(content);
                DiffInput::text(next)
            };
            let result = fs_ops.append_file(path, content)?;
            let diff = build_diff(before_snapshot, after_snapshot);
            let full_path = target.to_string_lossy().to_string();
            let record = change_log
                .lock()
                .map_err(|_| "change log unavailable".to_string())?
                .log_change(
                    &result.path,
                    "append",
                    if existed_before { "edit" } else { "create" },
                    result.bytes,
                    &result.sha256,
                    ctx.conversation_id,
                    ctx.run_id,
                    diff,
                )?;
            note_workspace_path_changed(hooks.as_ref(), full_path.as_str());
            Ok(text_result(json!({
                "outcome": FileModificationOutcome::Changed,
                "changed": true,
                "changed_target_count": 1,
                "previous_sha256": current_sha256,
                "result": result,
                "change": record
            })))
            })();
            record_file_modification_outcome("append_file", ctx, &invocation);
            invocation
        }),
    );
}

fn register_delete_path_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: Arc<Mutex<ModificationRevisionGuard>>,
    max_file_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    service.register_tool(
        "delete_path",
        &format!(
            "Delete a file or directory recursively from the current project workspace. expected_sha256 is mandatory for files and must come from the latest read; use null only for a directory or a path confirmed absent.\n{}.\n{workspace_note}",
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "expected_sha256": {
                    "type": ["string", "null"],
                    "pattern": "^[0-9a-f]{64}$"
                }
            },
            "additionalProperties": false,
            "required": ["path", "expected_sha256"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let expected_sha256 = expected_sha256(&args, "expected_sha256")?;
            let target = fs_ops.resolve_path(path)?;
            let current_sha256 = if target.is_file() {
                Some(fs_ops.read_file_raw(path)?.2)
            } else {
                None
            };
            verify_file_revision(
                &revision_guard,
                ctx,
                path,
                expected_sha256,
                current_sha256.as_deref(),
                None,
                None,
            )?;
            let before_snapshot =
                read_text_for_diff(&target, max_file_bytes).unwrap_or_else(DiffInput::omitted);
            let after_snapshot = if let Some(reason) = before_snapshot.reason.clone() {
                DiffInput::omitted(reason)
            } else {
                DiffInput::text(String::new())
            };
            let full_path = target.to_string_lossy().to_string();
            let delete_result = fs_ops.delete_path(path)?;
            let exists_after_delete = target.exists();
            if delete_result.deleted && exists_after_delete {
                return Err(format!(
                    "Delete reported success but path still exists: {}",
                    delete_result.path
                ));
            }
            if !delete_result.deleted {
                return Ok(text_result(json!({
                    "outcome": FileModificationOutcome::AlreadyApplied,
                    "changed": false,
                    "changed_target_count": 0,
                    "result": {
                        "path": delete_result.path,
                        "deleted": false,
                        "exists_after_delete": exists_after_delete,
                        "already_absent": true
                    },
                    "message": "Path already absent. No file-system change was applied.",
                    "hint": "Verify the exact path with list_dir before retrying delete."
                })));
            }
            let diff = build_diff(before_snapshot, after_snapshot);
            let record = change_log
                .lock()
                .map_err(|_| "change log unavailable".to_string())?
                .log_change(
                    &delete_result.path,
                    "delete",
                    "delete",
                    0,
                    "",
                    ctx.conversation_id,
                    ctx.run_id,
                    diff,
                )?;
            note_workspace_path_changed(hooks.as_ref(), full_path.as_str());
            Ok(text_result(json!({
                "outcome": FileModificationOutcome::Changed,
                "changed": true,
                "changed_target_count": 1,
                "result": {
                    "path": delete_result.path,
                    "deleted": true,
                    "exists_after_delete": exists_after_delete,
                    "already_absent": false
                },
                "change": record
            })))
            })();
            record_file_modification_outcome("delete_path", ctx, &invocation);
            invocation
        }),
    );
}

fn register_apply_patch_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    revision_guard: Arc<Mutex<ModificationRevisionGuard>>,
    root: PathBuf,
    allow_writes: bool,
    max_file_bytes: i64,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
    hooks: Option<CodeMaintainerHooksRef>,
) {
    service.register_tool(
        "apply_patch",
        &format!(
            "Apply a patch to one or more files in the current project workspace. expected_sha256_by_path is mandatory and must contain the latest read sha256 for every existing update/delete target; omit paths that the patch creates. After stale_context or expected_match, re-read every reported target before retrying.\nSupported format A (recommended): *** Begin Patch / *** Update File / *** Add File / *** Delete File / *** End Patch.\nSupported format B (stable text replace):\nUpdate File --- path/to/file\n<old content>\n+++ path/to/file\n<new content>\nEnd Patch\nFormat B requires old content to match uniquely in the file.\n{}.\n{workspace_note}",
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string", "minLength": 1 },
                "expected_sha256_by_path": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "string",
                        "pattern": "^[0-9a-f]{64}$"
                    }
                }
            },
            "additionalProperties": false,
            "required": ["patch", "expected_sha256_by_path"]
        }),
        Arc::new(move |args, ctx| {
            let invocation = (|| {
            let patch_text = args
                .get("patch")
                .and_then(|value| value.as_str())
                .ok_or("patch is required".to_string())?;
            let expected_hashes = args
                .get("expected_sha256_by_path")
                .and_then(serde_json::Value::as_object)
                .ok_or("expected_sha256_by_path is required".to_string())?;
            let patch_diffs: HashMap<String, String> = extract_patch_diffs(patch_text);
            let patch_targets = extract_patch_targets(patch_text);
            let mut before_snapshots: HashMap<String, DiffInput> = HashMap::new();
            let mut current_hashes = HashMap::new();
            let mut existing_paths = std::collections::BTreeSet::new();
            for target in &patch_targets {
                let before_path = fs_ops.resolve_path(&target.before_path)?;
                if before_path.exists() {
                    if !before_path.is_file() {
                        return Err(format!(
                            "patch target is not a file: {}",
                            target.before_path
                        ));
                    }
                    if existing_paths.insert(target.before_path.clone()) {
                        let (_, _, current_sha256, _) =
                            fs_ops.read_file_raw(target.before_path.as_str())?;
                        let expected_sha256 = expected_hashes
                            .get(target.before_path.as_str())
                            .and_then(serde_json::Value::as_str)
                            .filter(|value| is_sha256(value))
                            .ok_or_else(|| {
                                format!(
                                    "expected_sha256_by_path requires a valid SHA-256 for existing target {}",
                                    target.before_path
                                )
                            })?;
                        verify_file_revision(
                            &revision_guard,
                            ctx,
                            target.before_path.as_str(),
                            Some(expected_sha256),
                            Some(current_sha256.as_str()),
                            None,
                            None,
                        )?;
                        current_hashes.insert(target.before_path.clone(), current_sha256);
                    }
                }
                let before_snapshot =
                    read_text_for_diff(&before_path, max_file_bytes).unwrap_or_else(DiffInput::omitted);
                before_snapshots.insert(target.after_path.clone(), before_snapshot);
            }
            let unexpected_hash_paths = expected_hashes
                .keys()
                .filter(|path| !existing_paths.contains(path.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !unexpected_hash_paths.is_empty() {
                return Err(format!(
                    "expected_sha256_by_path contains paths that are not existing patch targets: {}",
                    unexpected_hash_paths.join(", ")
                ));
            }
            let result = apply_patch_limited(&root, patch_text, allow_writes, max_write_bytes).map_err(|err| {
                let outcome = classify_file_modification_error(err.as_str());
                if matches!(
                    outcome,
                    FileModificationOutcome::StaleContext
                        | FileModificationOutcome::ExpectedMatch
                ) {
                    for path in current_hashes.keys() {
                        mark_failed_modification(&revision_guard, ctx, path.as_str());
                    }
                    let first_path = current_hashes.keys().next().map(String::as_str).unwrap_or("");
                    file_revision_error(
                        outcome.as_str(),
                        err.as_str(),
                        first_path,
                        current_hashes.get(first_path).map(String::as_str),
                        None,
                        None,
                    )
                } else {
                    err
                }
            })?;
            let mut hashes = Vec::new();

            {
                let store = change_log
                    .lock()
                    .map_err(|_| "change log unavailable".to_string())?;
                for path in &result.updated {
                    let full_path = fs_ops.resolve_path(path)?;
                    let (hash, size) = file_hash_and_size(&full_path)?;
                    let before_snapshot = before_snapshots.remove(path).unwrap_or(DiffInput {
                        text: None,
                        reason: None,
                    });
                    let after_snapshot = read_text_for_diff(&full_path, max_file_bytes)
                        .unwrap_or_else(DiffInput::omitted);
                    let change_kind = if before_snapshot.text.is_none()
                        && before_snapshot.reason.is_none()
                    {
                        "create"
                    } else {
                        "edit"
                    };
                    let diff =
                        build_diff(before_snapshot, after_snapshot).or_else(|| patch_diffs.get(path).cloned());
                    store.log_change(
                        path,
                        "write",
                        change_kind,
                        size,
                        &hash,
                        ctx.conversation_id,
                        ctx.run_id,
                        diff,
                    )?;
                    let full_path_string = full_path.to_string_lossy().to_string();
                    note_workspace_path_changed(hooks.as_ref(), full_path_string.as_str());
                    hashes.push(json!({ "path": path, "sha256": hash }));
                }

                for path in &result.added {
                    let full_path = fs_ops.resolve_path(path)?;
                    let (hash, size) = file_hash_and_size(&full_path)?;
                    let before_snapshot = before_snapshots.remove(path).unwrap_or(DiffInput {
                        text: None,
                        reason: None,
                    });
                    let after_snapshot = read_text_for_diff(&full_path, max_file_bytes)
                        .unwrap_or_else(DiffInput::omitted);
                    let diff =
                        build_diff(before_snapshot, after_snapshot).or_else(|| patch_diffs.get(path).cloned());
                    store.log_change(
                        path,
                        "write",
                        "create",
                        size,
                        &hash,
                        ctx.conversation_id,
                        ctx.run_id,
                        diff,
                    )?;
                    let full_path_string = full_path.to_string_lossy().to_string();
                    note_workspace_path_changed(hooks.as_ref(), full_path_string.as_str());
                    hashes.push(json!({ "path": path, "sha256": hash }));
                }

                for path in &result.deleted {
                    let full_path = fs_ops.resolve_path(path)?;
                    let before_snapshot = before_snapshots
                        .remove(path)
                        .unwrap_or_else(|| DiffInput::text(String::new()));
                    let after_snapshot = DiffInput::text(String::new());
                    let diff =
                        build_diff(before_snapshot, after_snapshot).or_else(|| patch_diffs.get(path).cloned());
                    store.log_change(
                        path,
                        "delete",
                        "delete",
                        0,
                        "",
                        ctx.conversation_id,
                        ctx.run_id,
                        diff,
                    )?;
                    let full_path_string = full_path.to_string_lossy().to_string();
                    note_workspace_path_changed(hooks.as_ref(), full_path_string.as_str());
                }
            }

            let changed = result.changed();
            let changed_target_count = result.changed_path_count();
            Ok(text_result(json!({
                "outcome": FileModificationOutcome::from_changed(changed),
                "changed": changed,
                "changed_target_count": changed_target_count,
                "result": result,
                "files": hashes
            })))
            })();
            record_file_modification_outcome("apply_patch", ctx, &invocation);
            invocation
        }),
    );
}

fn expected_sha256<'a>(
    args: &'a serde_json::Value,
    field: &str,
) -> Result<Option<&'a str>, String> {
    match args.get(field) {
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) if is_sha256(value) => Ok(Some(value.as_str())),
        Some(serde_json::Value::String(_)) => Err(format!(
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

fn verify_file_revision(
    revision_guard: &Arc<Mutex<ModificationRevisionGuard>>,
    ctx: &super::service::ToolContext<'_>,
    path: &str,
    expected: Option<&str>,
    current: Option<&str>,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<(), String> {
    let mut guard = revision_guard
        .lock()
        .map_err(|_| "file revision guard unavailable".to_string())?;
    if guard.is_reread_required(ctx.run_id, path) {
        return Err(file_revision_error(
            "stale_context",
            "A successful file read is required after the previous failed modification",
            path,
            current,
            start_line,
            end_line,
        ));
    }
    if expected == current {
        return Ok(());
    }
    if current.is_some() {
        guard.require_reread(ctx.run_id, path);
    }
    Err(file_revision_error(
        "stale_context",
        "The target file revision does not match the modification request",
        path,
        current,
        start_line,
        end_line,
    ))
}

fn mark_failed_modification(
    revision_guard: &Arc<Mutex<ModificationRevisionGuard>>,
    ctx: &super::service::ToolContext<'_>,
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
            "required_next_tool": if start_line.is_some() || end_line.is_some() {
                "read_file_range"
            } else {
                "read_file_raw"
            },
            "recommended_args": {
                "path": path,
                "start_line": start_line,
                "end_line": end_line,
            },
            "guidance": "Re-read the target, use the returned sha256 as expected_sha256, then rebuild the modification from the current content."
        }
    }))
    .unwrap_or_else(|_| format!("{category}: {message}"))
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
    );
    let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(base.as_str()) else {
        return base;
    };
    payload["candidate_summary"] = edit_candidate_summary(content, old_text);
    serde_json::to_string(&payload).unwrap_or(base)
}

fn edit_candidate_summary(content: &str, old_text: &str) -> serde_json::Value {
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

fn file_hash_and_size(path: &Path) -> Result<(String, i64), String> {
    let mut file = std::fs::File::open(path).map_err(|err| err.to_string())?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|err| err.to_string())?;
        if read == 0 {
            let size = i64::try_from(total).map_err(|_| "file too large to log".to_string())?;
            return Ok((hex::encode(hasher.finalize()), size));
        }
        total = total.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
}

fn note_workspace_path_changed(hooks: Option<&CodeMaintainerHooksRef>, path: &str) {
    if let Some(hooks) = hooks {
        hooks.note_workspace_path_changed(path);
    }
}

fn record_file_modification_outcome(
    tool: &str,
    ctx: &super::service::ToolContext<'_>,
    invocation: &Result<serde_json::Value, String>,
) {
    let (outcome, success, changed, changed_target_count) = match invocation {
        Ok(value) => {
            let payload = value.get("_structured_result").unwrap_or(value);
            let changed = payload
                .get("changed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let outcome = payload
                .get("outcome")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| FileModificationOutcome::from_changed(changed).as_str());
            let changed_target_count = payload
                .get("changed_target_count")
                .and_then(serde_json::Value::as_u64)
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
