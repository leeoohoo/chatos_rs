use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::diff::{
    build_diff, extract_patch_diffs, extract_patch_targets, read_text_for_diff, DiffInput,
};
use super::edit::{apply_edit_text, EditRequest};
use super::fs_ops::FsOps;
use super::patch::apply_patch;
use super::service::CodeMaintainerService;
use super::storage::ChangeLogStore;
use super::utils::{format_bytes, sha256_bytes};

use crate::core::tool_io::text_result;
use crate::services::workspace_realtime_watcher::{
    note_workspace_path_changed, suppress_logged_path,
};

pub(super) fn register_write_tools(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    root: PathBuf,
    allow_writes: bool,
    max_file_bytes: i64,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
) {
    register_write_file_tool(
        service,
        fs_ops.clone(),
        change_log.clone(),
        max_file_bytes,
        max_write_bytes,
        writes_note,
        workspace_note,
    );
    register_edit_file_tool(service, fs_ops.clone(), change_log.clone(), workspace_note);
    register_append_file_tool(
        service,
        fs_ops.clone(),
        change_log.clone(),
        max_file_bytes,
        max_write_bytes,
        writes_note,
        workspace_note,
    );
    register_delete_path_tool(
        service,
        fs_ops.clone(),
        change_log.clone(),
        max_file_bytes,
        writes_note,
        workspace_note,
    );
    register_apply_patch_tool(
        service,
        fs_ops,
        change_log,
        root,
        allow_writes,
        max_file_bytes,
        writes_note,
        workspace_note,
    );
}

fn register_write_file_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    max_file_bytes: i64,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
) {
    service.register_tool(
        "write_file",
        &format!(
            "Write file content (overwrite).\nMax write bytes: {}.\n{}.\n{workspace_note}",
            format_bytes(max_write_bytes),
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "additionalProperties": false,
            "required": ["path", "content"]
        }),
        Arc::new(move |args, ctx| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let content = args
                .get("content")
                .and_then(|value| value.as_str())
                .ok_or("content is required".to_string())?;
            let target = fs_ops.resolve_path(path)?;
            let existed_before = target.exists();
            let before_snapshot =
                read_text_for_diff(&target, max_file_bytes).unwrap_or_else(DiffInput::omitted);
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
            suppress_logged_path(full_path.as_str());
            note_workspace_path_changed(full_path.as_str());
            Ok(text_result(json!({ "result": result, "change": record })))
        }),
    );
}

fn register_edit_file_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    workspace_note: &str,
) {
    service.register_tool(
        "edit_file",
        &format!(
            "Safely edit file content by replacing old_text with new_text.\nWhen old_text appears multiple times, you MUST provide more surrounding context (before_context / after_context, recommended 1-3 lines) or narrow start_line/end_line.\n{workspace_note}"
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
                "expected_matches": { "type": "integer", "minimum": 1 }
            },
            "additionalProperties": false,
            "required": ["path", "old_text", "new_text"]
        }),
        Arc::new(move |args, ctx| {
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

            let (_resolved_path, _size, _sha, content) = fs_ops.read_file_raw(path)?;
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
                if err.contains("old_text not found in file.") || err.contains("expected_matches mismatch") {
                    let hint = json!({
                        "error": err,
                        "recovery": {
                            "recommended_next_tools": [
                                "read_file_raw",
                                "read_file_range"
                            ],
                            "guidance": "File content likely changed. Re-read latest file content, then retry edit with tighter before_context/after_context or line range."
                        }
                    });
                    serde_json::to_string(&hint).unwrap_or(err)
                } else {
                    err
                }
            })?;

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
            suppress_logged_path(full_path.as_str());
            note_workspace_path_changed(full_path.as_str());
            Ok(text_result(json!({
                "result": write_result,
                "match": edit_result.info,
                "change": record
            })))
        }),
    );
}

fn register_append_file_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    max_file_bytes: i64,
    max_write_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
) {
    service.register_tool(
        "append_file",
        &format!(
            "Append content to file.\nMax write bytes: {}.\n{}.\n{workspace_note}",
            format_bytes(max_write_bytes),
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "additionalProperties": false,
            "required": ["path", "content"]
        }),
        Arc::new(move |args, ctx| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let content = args
                .get("content")
                .and_then(|value| value.as_str())
                .ok_or("content is required".to_string())?;
            let target = fs_ops.resolve_path(path)?;
            let existed_before = target.exists();
            let before_snapshot =
                read_text_for_diff(&target, max_file_bytes).unwrap_or_else(DiffInput::omitted);
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
            suppress_logged_path(full_path.as_str());
            note_workspace_path_changed(full_path.as_str());
            Ok(text_result(json!({ "result": result, "change": record })))
        }),
    );
}

fn register_delete_path_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    max_file_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
) {
    service.register_tool(
        "delete_path",
        &format!(
            "Delete a file or directory.\n{}.\n{workspace_note}",
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "additionalProperties": false,
            "required": ["path"]
        }),
        Arc::new(move |args, ctx| {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("path is required".to_string())?;
            let target = fs_ops.resolve_path(path)?;
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
            suppress_logged_path(full_path.as_str());
            note_workspace_path_changed(full_path.as_str());
            Ok(text_result(json!({
                "result": {
                    "path": delete_result.path,
                    "deleted": true,
                    "exists_after_delete": exists_after_delete,
                    "already_absent": false
                },
                "change": record
            })))
        }),
    );
}

fn register_apply_patch_tool(
    service: &mut CodeMaintainerService,
    fs_ops: FsOps,
    change_log: Arc<Mutex<ChangeLogStore>>,
    root: PathBuf,
    allow_writes: bool,
    max_file_bytes: i64,
    writes_note: &str,
    workspace_note: &str,
) {
    service.register_tool(
        "apply_patch",
        &format!(
            "Apply a patch to one or more files.\nSupported format A (recommended): *** Begin Patch / *** Update File / *** Add File / *** Delete File / *** End Patch.\nSupported format B (stable text replace):\nUpdate File --- path/to/file\n<old content>\n+++ path/to/file\n<new content>\nEnd Patch\nFormat B requires old content to match uniquely in the file.\n{}.\n{workspace_note}",
            writes_note
        ),
        json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string", "minLength": 1 }
            },
            "additionalProperties": false,
            "required": ["patch"]
        }),
        Arc::new(move |args, ctx| {
            let patch_text = args
                .get("patch")
                .and_then(|value| value.as_str())
                .ok_or("patch is required".to_string())?;
            let patch_diffs: HashMap<String, String> = extract_patch_diffs(patch_text);
            let patch_targets = extract_patch_targets(patch_text);
            let mut before_snapshots: HashMap<String, DiffInput> = HashMap::new();
            for target in patch_targets {
                let before_path = fs_ops.resolve_path(&target.before_path)?;
                let before_snapshot =
                    read_text_for_diff(&before_path, max_file_bytes).unwrap_or_else(DiffInput::omitted);
                before_snapshots.insert(target.after_path, before_snapshot);
            }
            let result = apply_patch(&root, patch_text, allow_writes).map_err(|err| {
                if err.contains("Patch context not found in file.") {
                    let hint = json!({
                        "error": err,
                        "recovery": {
                            "recommended_next_tools": [
                                "read_file_raw",
                                "read_file_range"
                            ],
                            "guidance": "Patch context is stale. Re-read target files and regenerate patch with exact current lines."
                        }
                    });
                    serde_json::to_string(&hint).unwrap_or(err)
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
                    let content = std::fs::read(&full_path).map_err(|err| err.to_string())?;
                    let hash = sha256_bytes(&content);
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
                        content.len() as i64,
                        &hash,
                        ctx.conversation_id,
                        ctx.run_id,
                        diff,
                    )?;
                    let full_path_string = full_path.to_string_lossy().to_string();
                    suppress_logged_path(full_path_string.as_str());
                    note_workspace_path_changed(full_path_string.as_str());
                    hashes.push(json!({ "path": path, "sha256": hash }));
                }

                for path in &result.added {
                    let full_path = fs_ops.resolve_path(path)?;
                    let content = std::fs::read(&full_path).map_err(|err| err.to_string())?;
                    let hash = sha256_bytes(&content);
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
                        content.len() as i64,
                        &hash,
                        ctx.conversation_id,
                        ctx.run_id,
                        diff,
                    )?;
                    let full_path_string = full_path.to_string_lossy().to_string();
                    suppress_logged_path(full_path_string.as_str());
                    note_workspace_path_changed(full_path_string.as_str());
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
                    suppress_logged_path(full_path_string.as_str());
                    note_workspace_path_changed(full_path_string.as_str());
                }
            }

            Ok(text_result(json!({ "result": result, "files": hashes })))
        }),
    );
}
