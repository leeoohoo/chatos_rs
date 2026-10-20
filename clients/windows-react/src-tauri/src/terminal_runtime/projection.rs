use super::process::{ManagedProcess, ProcessSnapshot, TerminalLog};
use serde_json::{json, Value};
use std::path::Path;

const MAXIMUM_OUTPUT_CHARACTERS: usize = 512 * 1_024;
const MAXIMUM_PREVIEW_CHARACTERS: usize = 1_200;

pub(super) async fn background_result(process: &ManagedProcess) -> Value {
    let snapshot = process.snapshot().await;
    json!({
        "project_root": ".",
        "terminal_id": snapshot.id,
        "process_id": snapshot.id,
        "terminal_reused": false,
        "path": display_path(&snapshot.cwd, &snapshot.project_root),
        "common": snapshot.command,
        "background": true,
        "busy": true,
        "output": "",
        "output_chars": 0,
        "truncated": false,
        "finished_by": "background"
    })
}

pub(super) async fn foreground_result(
    process: &ManagedProcess,
    timed_out: bool,
    timeout_ms: u64,
) -> Value {
    let snapshot = process.snapshot().await;
    let stdout = combined_output(&snapshot, &["stdout"], MAXIMUM_OUTPUT_CHARACTERS);
    let stderr = combined_output(&snapshot, &["stderr"], MAXIMUM_OUTPUT_CHARACTERS);
    let combined = combined_output(&snapshot, &["stdout", "stderr"], MAXIMUM_OUTPUT_CHARACTERS);
    json!({
        "project_root": ".",
        "terminal_id": snapshot.id,
        "process_id": snapshot.id,
        "terminal_reused": false,
        "path": display_path(&snapshot.cwd, &snapshot.project_root),
        "common": snapshot.command,
        "background": false,
        "busy": snapshot.status != "exited",
        "success": !timed_out && snapshot.exit_code == Some(0),
        "timed_out": timed_out,
        "timeout_ms": timeout_ms,
        "stdout": stdout.text,
        "stderr": stderr.text,
        "output": combined.text,
        "output_chars": combined.characters,
        "truncated": combined.truncated,
        "finished_by": if timed_out { "timeout" } else { "exit" },
        "error": timed_out.then(|| format!(
            "前台命令在 {} 秒内未结束，已终止。预计运行更久的命令请使用 background=true，并通过 process_poll 或 process_wait 跟踪。",
            timeout_ms / 1_000
        )),
        "exit_code": snapshot.exit_code
    })
}

pub(super) async fn poll_result(
    process: &ManagedProcess,
    offset: Option<u64>,
    limit: usize,
) -> Value {
    let snapshot = process.snapshot().await;
    let selected = selected_logs(&snapshot, offset, limit);
    let first_available_offset = snapshot
        .logs
        .first()
        .map(|log| log.offset)
        .unwrap_or(snapshot.next_log_offset);
    let requested = offset.unwrap_or(first_available_offset);
    let truncated = snapshot.logs_truncated
        && (snapshot.output_truncated || requested < first_available_offset);
    let next_offset = selected.last().map(|log| log.offset.saturating_add(1));
    let has_more =
        next_offset.is_some_and(|next| snapshot.logs.iter().any(|log| log.offset >= next));
    let mut value = process_snapshot(&snapshot);
    let fields = value
        .as_object_mut()
        .expect("process snapshot is an object");
    fields.insert(
        "mode".into(),
        json!(if offset.is_some() { "offset" } else { "recent" }),
    );
    fields.insert("requested_offset".into(), json!(offset));
    fields.insert("next_offset".into(), json!(next_offset));
    fields.insert("limit".into(), json!(limit));
    fields.insert("fetched_log_count".into(), json!(selected.len()));
    fields.insert("returned_log_count".into(), json!(selected.len()));
    fields.insert("has_more".into(), json!(has_more));
    fields.insert("truncated".into(), json!(truncated));
    fields.insert(
        "truncation".into(),
        json!({
            "truncated": truncated,
            "first_available_offset": first_available_offset
        }),
    );
    fields.insert(
        "logs".into(),
        Value::Array(selected.iter().map(log_value).collect()),
    );
    value
}

pub(super) async fn log_result(
    process: &ManagedProcess,
    offset: Option<u64>,
    limit: usize,
) -> Value {
    let polled = poll_result(process, offset, limit).await;
    let fields = polled.as_object().expect("poll result is an object");
    let output = fields
        .get("logs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|log| log.get("content").and_then(Value::as_str))
        .collect::<String>();
    json!({
        "terminal_id": fields.get("terminal_id").cloned().unwrap_or(Value::Null),
        "status": fields.get("status").cloned().unwrap_or(json!("unknown")),
        "output": output,
        "offset": offset,
        "limit": limit,
        "has_more": fields.get("has_more").cloned().unwrap_or(json!(false)),
        "next_offset": fields.get("next_offset").cloned().unwrap_or(Value::Null),
        "truncated": fields.get("truncated").cloned().unwrap_or(json!(false)),
        "truncation": fields.get("truncation").cloned().unwrap_or(json!({"truncated": false}))
    })
}

pub(super) async fn wait_result(
    process: &ManagedProcess,
    timed_out: bool,
    timeout_ms: u64,
    waited_ms: i64,
) -> Value {
    let snapshot = process.snapshot().await;
    let output = combined_output(&snapshot, &["stdout", "stderr"], MAXIMUM_OUTPUT_CHARACTERS);
    json!({
        "terminal_id": snapshot.id,
        "process_id": snapshot.id,
        "terminal_name": terminal_name(&snapshot),
        "status": snapshot.status,
        "wait_status": if timed_out { "timeout" } else { "exited" },
        "busy": snapshot.status != "exited",
        "exited": snapshot.status == "exited",
        "completed": !timed_out,
        "timed_out": timed_out,
        "finished_by": if timed_out { "timeout" } else { "exit" },
        "exit_code": snapshot.exit_code,
        "timeout_ms": timeout_ms,
        "waited_ms": waited_ms,
        "output": output.text,
        "output_preview": output.text,
        "output_chars": output.characters,
        "truncated": output.truncated
    })
}

fn process_snapshot(snapshot: &ProcessSnapshot) -> Value {
    let output = combined_output(snapshot, &["stdout", "stderr"], MAXIMUM_PREVIEW_CHARACTERS);
    json!({
        "terminal_id": snapshot.id,
        "process_id": snapshot.id,
        "terminal_name": terminal_name(snapshot),
        "status": snapshot.status,
        "process_status": if snapshot.status == "exited" { "exited" } else { "running" },
        "busy": snapshot.status != "exited",
        "has_session": true,
        "command": snapshot.command,
        "started_at_unix_ms": snapshot.started_at_unix_ms,
        "last_active_at_unix_ms": snapshot.last_active_at_unix_ms,
        "cwd": display_path(&snapshot.cwd, &snapshot.project_root),
        "output_preview": output.text,
        "output_tail": output.text,
        "output_tail_chars": output.characters,
        "exit_code": snapshot.exit_code
    })
}

fn selected_logs(
    snapshot: &ProcessSnapshot,
    offset: Option<u64>,
    limit: usize,
) -> Vec<TerminalLog> {
    match offset {
        Some(offset) => snapshot
            .logs
            .iter()
            .filter(|log| log.offset >= offset)
            .take(limit)
            .cloned()
            .collect(),
        None => snapshot
            .logs
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
    }
}

fn log_value(log: &TerminalLog) -> Value {
    json!({
        "offset": log.offset,
        "kind": log.kind,
        "content": log.content,
        "created_at_unix_ms": log.created_at_unix_ms
    })
}

fn combined_output(snapshot: &ProcessSnapshot, kinds: &[&str], maximum: usize) -> CombinedOutput {
    let full = snapshot
        .logs
        .iter()
        .filter(|log| kinds.contains(&log.kind))
        .map(|log| log.content.as_str())
        .collect::<String>();
    let characters = full.chars().count();
    if characters <= maximum {
        return CombinedOutput {
            text: full,
            characters,
            truncated: snapshot.output_truncated,
        };
    }
    CombinedOutput {
        text: full
            .chars()
            .rev()
            .take(maximum)
            .collect::<String>()
            .chars()
            .rev()
            .collect(),
        characters,
        truncated: true,
    }
}

fn display_path(path: &Path, root: &Path) -> String {
    if path == root {
        return ".".into();
    }
    path.strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(".")
        .replace('\\', "/")
}

fn terminal_name(snapshot: &ProcessSnapshot) -> String {
    snapshot
        .cwd
        .file_name()
        .or_else(|| snapshot.project_root.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("terminal")
        .to_owned()
}

struct CombinedOutput {
    text: String,
    characters: usize,
    truncated: bool,
}
