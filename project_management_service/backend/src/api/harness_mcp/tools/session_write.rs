// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chatos_mcp::code_maintainer::FileModificationOutcome;
use serde_json::{json, Value};

use super::super::client::{
    commit_file_actions, ensure_action_count, fetch_harness_content, list_harness_paths,
    read_harness_file, sha256_hex, HarnessCommitAction, HarnessFile,
};
use super::super::path_policy::{optional_repo_path, path_matches_scope};
use super::super::session::{
    DirectoryFileSnapshot, EditSession, EntrySnapshot, FileSnapshot, SessionEntryState,
};
use super::super::text_edit::apply_text_edit;
use super::super::{ensure_write_size, required_string, tool_text_result, HarnessMcpContext};

pub(in super::super) async fn tool_open_edit_session(
    ctx: &HarnessMcpContext,
    _args: &Value,
) -> Result<Value, String> {
    let handle = ctx
        .session_store
        .lock()
        .await
        .open_session(ctx.project_id.as_str(), ctx.run_id.as_deref());
    Ok(tool_text_result(json!({
        "outcome": FileModificationOutcome::AlreadyApplied,
        "changed": false,
        "changed_target_count": 0,
        "result": handle.to_json(),
        "message": "Edit session opened. Stage batches against this session before committing."
    })))
}

pub(in super::super) async fn tool_stage_edit_batch(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let session_id = required_string(args, "session_id")?;
    let operations = args
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| "operations is required".to_string())?;
    if operations.is_empty() {
        return Err("operations must contain at least one item".to_string());
    }

    let mut store = ctx.session_store.lock().await;
    let session = store.get_mut(session_id, ctx.project_id.as_str(), ctx.run_id.as_deref())?;
    let mut staged_session = session.clone();
    let mut batch_changed_paths = BTreeSet::new();
    let mut batch_matches = Vec::new();
    for operation in operations {
        let outcome = apply_stage_operation(ctx, &mut staged_session, operation).await?;
        if outcome.changed {
            batch_changed_paths.insert(outcome.path.clone());
        }
        if let Some(info) = outcome.match_info {
            batch_matches.push(json!({ "path": outcome.path, "match": info }));
        }
    }
    staged_session.staged_operation_count += operations.len();
    staged_session.touch();
    let pending_paths = staged_session
        .changed_entries()
        .into_iter()
        .map(session_entry_summary)
        .collect::<Vec<_>>();
    let changed = !batch_changed_paths.is_empty();
    *session = staged_session;

    Ok(tool_text_result(json!({
        "outcome": FileModificationOutcome::from_changed(changed),
        "changed": changed,
        "changed_target_count": batch_changed_paths.len(),
        "result": {
            "session_id": session.id,
            "staged_operation_count": session.staged_operation_count,
            "batch_operation_count": operations.len(),
            "batch_changed_paths": batch_changed_paths.into_iter().collect::<Vec<_>>(),
            "pending_target_count": pending_paths.len(),
            "pending_paths": pending_paths,
        },
        "matches": batch_matches,
    })))
}

pub(in super::super) async fn tool_commit_edit_session(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let session_id = required_string(args, "session_id")?;
    let mut store = ctx.session_store.lock().await;
    let session = store.take(session_id, ctx.project_id.as_str(), ctx.run_id.as_deref())?;
    commit_session(ctx, session).await
}

pub(in super::super) async fn tool_abort_edit_session(
    ctx: &HarnessMcpContext,
    args: &Value,
) -> Result<Value, String> {
    let session_id = required_string(args, "session_id")?;
    let session = ctx.session_store.lock().await.take(
        session_id,
        ctx.project_id.as_str(),
        ctx.run_id.as_deref(),
    )?;
    let discarded_paths = session
        .changed_entries()
        .into_iter()
        .map(|state| state.path.clone())
        .collect::<Vec<_>>();
    Ok(tool_text_result(json!({
        "outcome": FileModificationOutcome::AlreadyApplied,
        "changed": false,
        "changed_target_count": 0,
        "result": {
            "session_id": session.id,
            "session_closed": true,
            "discarded_operation_count": session.staged_operation_count,
            "discarded_paths": discarded_paths,
        },
        "message": "Edit session aborted. No project files were changed."
    })))
}

struct StageOutcome {
    path: String,
    changed: bool,
    match_info: Option<Value>,
}

async fn apply_stage_operation(
    ctx: &HarnessMcpContext,
    session: &mut EditSession,
    operation: &Value,
) -> Result<StageOutcome, String> {
    let kind = required_string(operation, "kind")?;
    let path = optional_repo_path(operation.get("path").and_then(Value::as_str), false)?;
    validate_session_path_overlaps(session, path.as_str(), kind)?;
    match kind {
        "write" => stage_write(ctx, session, operation, path.as_str()).await,
        "replace_text" => stage_replace_text(ctx, session, operation, path.as_str()).await,
        "append" => stage_append(ctx, session, operation, path.as_str()).await,
        "delete" => stage_delete(ctx, session, operation, path.as_str()).await,
        other => Err(format!("unsupported operation kind: {other}")),
    }
}

async fn stage_write(
    ctx: &HarnessMcpContext,
    session: &mut EditSession,
    operation: &Value,
    path: &str,
) -> Result<StageOutcome, String> {
    let content = required_string(operation, "content")?.to_string();
    ensure_write_size(content.as_str())?;
    let expected = expected_revision(operation, "expected_sha256")?;
    let state = get_or_load_session_entry(ctx, session, path, expected).await?;
    if matches!(state.working, EntrySnapshot::Directory { .. }) {
        return Err("Target path is a directory.".to_string());
    }
    let next = staged_file(content);
    let changed = state.working != next;
    state.working = next;
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: path.to_string(),
        changed,
        match_info: None,
    })
}

async fn stage_replace_text(
    ctx: &HarnessMcpContext,
    session: &mut EditSession,
    operation: &Value,
    path: &str,
) -> Result<StageOutcome, String> {
    let old_text = required_string(operation, "old_text")?;
    let new_text = operation
        .get("new_text")
        .and_then(Value::as_str)
        .ok_or_else(|| "new_text is required".to_string())?;
    let expected = expected_revision(operation, "expected_sha256")?;
    let state = get_or_load_session_entry(ctx, session, path, expected).await?;
    let EntrySnapshot::File(current_file) = &state.working else {
        return Err("Target is not a file.".to_string());
    };
    let edit = apply_text_edit(current_file.content.as_str(), operation, old_text, new_text)?;
    ensure_write_size(edit.content.as_str())?;
    let changed = edit.changed;
    state.working = staged_file(edit.content);
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: path.to_string(),
        changed,
        match_info: Some(edit.info),
    })
}

async fn stage_append(
    ctx: &HarnessMcpContext,
    session: &mut EditSession,
    operation: &Value,
    path: &str,
) -> Result<StageOutcome, String> {
    let content = required_string(operation, "content")?;
    let expected = expected_revision(operation, "expected_sha256")?;
    let state = get_or_load_session_entry(ctx, session, path, expected).await?;
    if matches!(state.working, EntrySnapshot::Directory { .. }) {
        return Err("Target path is a directory.".to_string());
    }
    let mut next = match &state.working {
        EntrySnapshot::File(file) => file.content.clone(),
        EntrySnapshot::Missing => String::new(),
        EntrySnapshot::Directory { .. } => unreachable!(),
    };
    next.push_str(content);
    ensure_write_size(next.as_str())?;
    let next = staged_file(next);
    let changed = state.working != next;
    state.working = next;
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: path.to_string(),
        changed,
        match_info: None,
    })
}

async fn stage_delete(
    ctx: &HarnessMcpContext,
    session: &mut EditSession,
    operation: &Value,
    path: &str,
) -> Result<StageOutcome, String> {
    let expected = expected_revision(operation, "expected_sha256")?;
    let state = get_or_load_session_entry(ctx, session, path, expected).await?;
    let changed = !matches!(state.working, EntrySnapshot::Missing);
    state.working = EntrySnapshot::Missing;
    state.staged_operations += 1;
    Ok(StageOutcome {
        path: path.to_string(),
        changed,
        match_info: None,
    })
}

async fn get_or_load_session_entry<'a>(
    ctx: &HarnessMcpContext,
    session: &'a mut EditSession,
    path: &str,
    expected: ExpectedRevision<'_>,
) -> Result<&'a mut SessionEntryState, String> {
    if !session.entries.contains_key(path) {
        let expected = expected
            .into_value()
            .ok_or_else(|| "expected_sha256 is required when a path is first staged".to_string())?;
        let snapshot = load_entry_snapshot(ctx, path).await?;
        verify_revision(path, expected, &snapshot)?;
        session
            .entries
            .insert(path.to_string(), SessionEntryState::new(path, snapshot));
    } else if let Some(expected) = expected.into_value() {
        let state = session
            .entries
            .get(path)
            .ok_or_else(|| format!("session path unexpectedly missing: {path}"))?;
        verify_revision(path, expected, &state.base)?;
    }
    session
        .entries
        .get_mut(path)
        .ok_or_else(|| format!("session path unexpectedly missing: {path}"))
}

async fn load_entry_snapshot(ctx: &HarnessMcpContext, path: &str) -> Result<EntrySnapshot, String> {
    match fetch_harness_content(ctx, path).await {
        Ok(content) if content.kind == "file" => {
            let file = read_harness_file(ctx, path).await?;
            Ok(file_snapshot(file))
        }
        Ok(content) if content.kind == "dir" || content.kind == "directory" => {
            load_directory_snapshot(ctx, path).await
        }
        Ok(content) => Err(format!("Unsupported project entry type: {}", content.kind)),
        Err(error) if error.is_not_found() => Ok(EntrySnapshot::Missing),
        Err(error) => Err(error.to_string()),
    }
}

async fn load_directory_snapshot(
    ctx: &HarnessMcpContext,
    path: &str,
) -> Result<EntrySnapshot, String> {
    let paths = list_harness_paths(ctx).await?;
    let candidates = paths
        .files
        .into_iter()
        .filter(|candidate| path_matches_scope(candidate, path) && candidate != path)
        .collect::<Vec<_>>();
    ensure_action_count(candidates.len())?;
    let mut files = BTreeMap::new();
    for candidate in candidates {
        match read_harness_file(ctx, candidate.as_str()).await {
            Ok(file) => {
                files.insert(
                    candidate,
                    DirectoryFileSnapshot {
                        sha256: file.sha256,
                        harness_blob_sha: file.harness_blob_sha,
                    },
                );
            }
            Err(error) if error == "Target is not a file." => {}
            Err(error) => return Err(error),
        }
    }
    Ok(EntrySnapshot::Directory { files })
}

fn file_snapshot(file: HarnessFile) -> EntrySnapshot {
    EntrySnapshot::File(FileSnapshot {
        content: file.content,
        sha256: file.sha256,
        harness_blob_sha: file.harness_blob_sha,
        size: file.size,
    })
}

fn staged_file(content: String) -> EntrySnapshot {
    let sha256 = sha256_hex(content.as_bytes());
    EntrySnapshot::File(FileSnapshot {
        size: content.len() as i64,
        content,
        sha256,
        harness_blob_sha: String::new(),
    })
}

async fn commit_session(ctx: &HarnessMcpContext, session: EditSession) -> Result<Value, String> {
    let changed_entries = session.changed_entries();
    if changed_entries.is_empty() {
        return Ok(tool_text_result(json!({
            "outcome": FileModificationOutcome::AlreadyApplied,
            "changed": false,
            "changed_target_count": 0,
            "result": {
                "session_id": session.id,
                "session_closed": true,
                "staged_operation_count": session.staged_operation_count,
                "committed_paths": [],
            },
            "message": "Session had no pending project file changes. Nothing was committed."
        })));
    }

    let mut conflicts = Vec::new();
    for state in &changed_entries {
        let current = load_entry_snapshot(ctx, state.path.as_str()).await?;
        if current != state.base {
            conflicts.push(conflict_payload(state, &current));
        }
    }
    if !conflicts.is_empty() {
        return Err(commit_conflict_error(conflicts.as_slice()));
    }

    let mut actions_by_path = BTreeMap::new();
    for state in changed_entries {
        append_commit_actions(&mut actions_by_path, state)?;
    }
    let actions = actions_by_path.into_values().collect::<Vec<_>>();
    ensure_action_count(actions.len())?;
    if actions.is_empty() {
        return Ok(tool_text_result(json!({
            "outcome": FileModificationOutcome::AlreadyApplied,
            "changed": false,
            "changed_target_count": 0,
            "result": {
                "session_id": session.id,
                "session_closed": true,
                "staged_operation_count": session.staged_operation_count,
                "committed_paths": [],
            }
        })));
    }

    let committed_paths = actions
        .iter()
        .map(|action| action.path.clone())
        .collect::<Vec<_>>();
    commit_file_actions(ctx, "Chatos: commit edit session", actions).await?;
    Ok(tool_text_result(json!({
        "outcome": FileModificationOutcome::Changed,
        "changed": true,
        "changed_target_count": committed_paths.len(),
        "result": {
            "session_id": session.id,
            "session_closed": true,
            "staged_operation_count": session.staged_operation_count,
            "committed_paths": committed_paths,
        }
    })))
}

fn append_commit_actions(
    actions: &mut BTreeMap<String, HarnessCommitAction>,
    state: &SessionEntryState,
) -> Result<(), String> {
    match (&state.base, &state.working) {
        (EntrySnapshot::Missing, EntrySnapshot::File(file)) => insert_action(
            actions,
            HarnessCommitAction {
                action: "CREATE".to_string(),
                path: state.path.clone(),
                payload: Some(file.content.clone()),
                encoding: Some("utf8".to_string()),
                sha: None,
            },
        ),
        (EntrySnapshot::File(base), EntrySnapshot::File(file)) => insert_action(
            actions,
            HarnessCommitAction {
                action: "UPDATE".to_string(),
                path: state.path.clone(),
                payload: Some(file.content.clone()),
                encoding: Some("utf8".to_string()),
                sha: non_empty(base.harness_blob_sha.as_str()),
            },
        ),
        (EntrySnapshot::File(base), EntrySnapshot::Missing) => insert_action(
            actions,
            HarnessCommitAction {
                action: "DELETE".to_string(),
                path: state.path.clone(),
                payload: None,
                encoding: None,
                sha: non_empty(base.harness_blob_sha.as_str()),
            },
        ),
        (EntrySnapshot::Directory { files }, EntrySnapshot::Missing) => {
            for (path, file) in files {
                insert_action(
                    actions,
                    HarnessCommitAction {
                        action: "DELETE".to_string(),
                        path: path.clone(),
                        payload: None,
                        encoding: None,
                        sha: non_empty(file.harness_blob_sha.as_str()),
                    },
                )?;
            }
            Ok(())
        }
        (base, working) if base == working => Ok(()),
        _ => Err(format!(
            "unsupported staged entry transition for {}: {} -> {}",
            state.path,
            state.base.kind_name(),
            state.working.kind_name()
        )),
    }
}

fn insert_action(
    actions: &mut BTreeMap<String, HarnessCommitAction>,
    action: HarnessCommitAction,
) -> Result<(), String> {
    if actions.insert(action.path.clone(), action).is_some() {
        Err("staged paths produce overlapping commit actions".to_string())
    } else {
        Ok(())
    }
}

fn verify_revision(
    path: &str,
    expected: Option<&str>,
    current: &EntrySnapshot,
) -> Result<(), String> {
    let current_sha = current.sha256();
    let matches = match current {
        EntrySnapshot::File(_) => expected == current_sha,
        EntrySnapshot::Missing | EntrySnapshot::Directory { .. } => expected.is_none(),
    };
    if matches {
        return Ok(());
    }
    Err(revision_error(path, current_sha))
}

fn revision_error(path: &str, latest_sha256: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "category": "stale_context",
        "error": "The target revision does not match the edit session baseline",
        "path": path,
        "latest_sha256": latest_sha256,
        "recovery": {
            "required_next_tool": "read_file_raw",
            "recommended_args": { "path": path },
            "guidance": "Re-read the target, open a fresh edit session, and restage against the latest content."
        }
    }))
    .unwrap_or_else(|_| "stale_context: file revision mismatch".to_string())
}

fn conflict_payload(state: &SessionEntryState, current: &EntrySnapshot) -> Value {
    json!({
        "path": state.path,
        "baseline_kind": state.base.kind_name(),
        "current_kind": current.kind_name(),
        "baseline_sha256": state.base.sha256(),
        "latest_sha256": current.sha256(),
    })
}

fn commit_conflict_error(conflicts: &[Value]) -> String {
    let first = conflicts.first().cloned().unwrap_or_else(|| json!({}));
    serde_json::to_string(&json!({
        "category": "stale_context",
        "error": "One or more staged paths changed after the session baseline was captured. The session was closed without applying any project file changes.",
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

fn validate_session_path_overlaps(
    session: &EditSession,
    path: &str,
    kind: &str,
) -> Result<(), String> {
    for existing in session.entries.values() {
        if existing.path == path {
            continue;
        }
        let nested = path.starts_with(format!("{}/", existing.path).as_str())
            || existing.path.starts_with(format!("{path}/").as_str());
        if nested
            && (kind == "delete"
                || matches!(existing.base, EntrySnapshot::Directory { .. })
                || matches!(existing.working, EntrySnapshot::Missing))
        {
            return Err(format!(
                "staged path overlaps another directory operation: {} and {}",
                existing.path, path
            ));
        }
    }
    Ok(())
}

fn session_entry_summary(state: &SessionEntryState) -> Value {
    json!({
        "path": state.path,
        "baseline_kind": state.base.kind_name(),
        "staged_kind": state.working.kind_name(),
        "changed": state.has_change(),
        "staged_operations": state.staged_operations,
        "staged_sha256": state.working_sha256(),
    })
}

enum ExpectedRevision<'a> {
    Omitted,
    Value(Option<&'a str>),
}

impl<'a> ExpectedRevision<'a> {
    fn into_value(self) -> Option<Option<&'a str>> {
        match self {
            Self::Omitted => None,
            Self::Value(value) => Some(value),
        }
    }
}

fn expected_revision<'a>(args: &'a Value, field: &str) -> Result<ExpectedRevision<'a>, String> {
    match args.get(field) {
        None => Ok(ExpectedRevision::Omitted),
        Some(Value::Null) => Ok(ExpectedRevision::Value(None)),
        Some(Value::String(value)) if is_sha256(value) => {
            Ok(ExpectedRevision::Value(Some(value.as_str())))
        }
        Some(Value::String(_)) => Err(format!(
            "{field} must be a lowercase 64-character SHA-256 value"
        )),
        Some(_) => Err(format!("{field} must be a SHA-256 string or null")),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_path_may_omit_revision_after_baseline_is_loaded() {
        assert!(matches!(
            expected_revision(&json!({}), "expected_sha256").unwrap(),
            ExpectedRevision::Omitted
        ));
    }

    #[test]
    fn commit_actions_collapse_ordered_same_file_edits_to_one_update() {
        let base = FileSnapshot {
            content: "before".to_string(),
            sha256: sha256_hex(b"before"),
            harness_blob_sha: "blob-1".to_string(),
            size: 6,
        };
        let state = SessionEntryState {
            path: "src/main.ts".to_string(),
            base: EntrySnapshot::File(base),
            working: staged_file("after".to_string()),
            staged_operations: 3,
        };
        let mut actions = BTreeMap::new();

        append_commit_actions(&mut actions, &state).expect("build actions");

        assert_eq!(actions.len(), 1);
        let action = actions.get("src/main.ts").expect("update action");
        assert_eq!(action.action, "UPDATE");
        assert_eq!(action.payload.as_deref(), Some("after"));
        assert_eq!(action.sha.as_deref(), Some("blob-1"));
    }

    #[test]
    fn revision_mismatch_is_structured_stale_context() {
        let current = staged_file("current".to_string());
        let error = verify_revision("src/main.ts", Some(&"a".repeat(64)), &current)
            .expect_err("stale revision");
        let payload: Value = serde_json::from_str(error.as_str()).expect("structured error");
        assert_eq!(payload["category"], "stale_context");
        assert_eq!(payload["path"], "src/main.ts");
    }
}
