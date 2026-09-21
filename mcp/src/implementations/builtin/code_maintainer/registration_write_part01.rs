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
        let resolved = fs_ops.resolve_write_path(state.path.as_str())?;
        let before_diff =
            read_text_for_diff(&resolved, max_file_bytes).unwrap_or_else(DiffInput::omitted);
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
                let bytes =
                    i64::try_from(content.len()).map_err(|_| "write too large".to_string())?;
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
    CreatedFile {
        path: PathBuf,
    },
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
            Self::ReplacedFile {
                backup, created, ..
            } => {
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

fn apply_path_commit(
    state: &SessionFileState,
    resolved: &Path,
) -> Result<PathCommitResult, String> {
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

fn apply_file_commit(
    state: &SessionFileState,
    resolved: &Path,
) -> Result<PathCommitResult, String> {
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
    if state.base.kind == current.kind
        && (state.base.kind != EntryKind::File || state.base.sha256 == current.sha256)
    {
        return None;
    }
    if matches!(state.base.kind, EntryKind::File) || matches!(current.kind, EntryKind::File) {
        mark_failed_modification(revision_guard, ctx, state.path.as_str());
    }
    let recovery_tool =
        if matches!(state.base.kind, EntryKind::File) || matches!(current.kind, EntryKind::File) {
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
            || existing
                .path
                .starts_with(format!("{}/", normalized).as_str());
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
    if content.len() as i64 > max_write_bytes {
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
            if expected.is_some()
                && guard.latest_read_matches(ctx.run_id, path, current.sha256.as_deref())
            {
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
            "next_session": {
                "tool": "open_edit_session",
                "args": { "fresh": true }
            },
            "guidance": "Read the current workspace state again, then call open_edit_session with fresh=true before rebuilding the staged batch from the latest content."
        }
    }))
    .unwrap_or_else(|_| format!("{category}: {message}"))
}

fn session_baseline_mismatch_error(path: &str, latest_sha256: Option<&str>) -> String {
    file_revision_error(
        "stale_context",
        "The expected revision does not match the active session baseline or staged snapshot",
        path,
        latest_sha256,
        None,
        None,
        "read_file_raw",
    )
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
            "next_session": {
                "tool": "open_edit_session",
                "args": { "fresh": true }
            },
            "guidance": "Re-read every conflicted path, call open_edit_session with fresh=true, and restage the batch against the latest content."
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
