// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub struct TerminalRecentLogsEntry {
    pub terminal_id: String,
    pub terminal_name: String,
    pub status: String,
    pub cwd: String,
    pub project_id: Option<String>,
    pub last_active_at: String,
    pub log_count: usize,
    pub logs: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct TerminalProcessSnapshot {
    pub terminal_id: String,
    pub terminal_name: String,
    pub status: String,
    pub process_status: String,
    pub busy: bool,
    pub command: String,
    pub started_at: String,
    pub cwd: String,
    pub project_id: Option<String>,
    pub last_active_at: String,
    pub output_preview: String,
    pub output_tail_chars: usize,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct TerminalProcessPollDetails {
    pub offset: Option<i64>,
    pub limit: usize,
    pub has_more: bool,
    pub logs: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct TerminalProcessWaitResponse {
    pub terminal_id: String,
    pub terminal_name: String,
    pub status: String,
    pub wait_status: String,
    pub busy: bool,
    pub exited: bool,
    pub completed: bool,
    pub timed_out: bool,
    pub finished_by: String,
    pub exit_code: Option<i32>,
    pub timeout_ms: u64,
    pub waited_ms: u64,
    pub output: String,
    pub output_chars: usize,
    pub truncated: bool,
}

pub fn terminal_recent_logs_entry(entry: TerminalRecentLogsEntry) -> Value {
    let returned_log_count = entry.logs.len();
    json!({
        "terminal_id": entry.terminal_id,
        "terminal_name": entry.terminal_name,
        "status": entry.status,
        "cwd": entry.cwd,
        "project_id": entry.project_id,
        "last_active_at": entry.last_active_at,
        "log_count": entry.log_count,
        "returned_log_count": returned_log_count,
        "truncated": false,
        "truncation": { "truncated": false },
        "logs": entry.logs,
    })
}

pub fn terminal_recent_logs_response(
    terminals: Vec<Value>,
    total_terminals: usize,
    per_terminal_limit: i64,
    terminal_limit: usize,
) -> Value {
    let terminal_count = terminals.len();
    json!({
        "result_scope": terminal_result_scope(terminal_count),
        "is_multiple_terminals": terminal_count > 1,
        "terminal_count": terminal_count,
        "total_terminals": total_terminals,
        "per_terminal_limit": per_terminal_limit,
        "terminal_limit": terminal_limit,
        "terminals": terminals,
    })
}

pub fn terminal_process_list_entry(snapshot: TerminalProcessSnapshot) -> Value {
    Value::Object(terminal_process_snapshot_map(snapshot))
}

pub fn terminal_process_list_response(
    processes: Vec<Value>,
    include_exited: bool,
    limit: usize,
) -> Value {
    let process_count = processes.len();
    json!({
        "status": "ok",
        "result_scope": terminal_result_scope(process_count),
        "is_multiple_terminals": process_count > 1,
        "terminal_count": process_count,
        "process_count": process_count,
        "visible_total": process_count,
        "total_terminals": process_count,
        "include_exited": include_exited,
        "limit": limit,
        "terminals": processes.clone(),
        "processes": processes,
    })
}

pub fn terminal_process_poll_response(
    snapshot: TerminalProcessSnapshot,
    details: TerminalProcessPollDetails,
) -> Value {
    let mut map = terminal_process_snapshot_map(snapshot);
    let returned_log_count = details.logs.len();
    map.insert(
        "mode".to_string(),
        Value::String(if details.offset.is_some() {
            "offset".to_string()
        } else {
            "recent".to_string()
        }),
    );
    map.insert("requested_offset".to_string(), json!(details.offset));
    map.insert(
        "next_offset".to_string(),
        details
            .logs
            .last()
            .and_then(|value| value.get("offset"))
            .and_then(Value::as_i64)
            .map(|value| json!(value + 1))
            .unwrap_or(Value::Null),
    );
    map.insert("limit".to_string(), json!(details.limit));
    map.insert("fetched_log_count".to_string(), json!(returned_log_count));
    map.insert("returned_log_count".to_string(), json!(returned_log_count));
    map.insert("has_more".to_string(), json!(details.has_more));
    map.insert("truncated".to_string(), Value::Bool(false));
    map.insert("truncation".to_string(), json!({ "truncated": false }));
    map.insert("logs".to_string(), Value::Array(details.logs));
    Value::Object(map)
}

pub fn terminal_process_log_response(poll: &Value, offset: Option<i64>, limit: i64) -> Value {
    let output = poll
        .get("logs")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.get("content").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    json!({
        "terminal_id": poll.get("terminal_id").cloned().unwrap_or(Value::Null),
        "status": poll.get("status").cloned().unwrap_or(Value::String("unknown".to_string())),
        "output": output,
        "offset": offset,
        "limit": limit,
        "has_more": poll.get("has_more").cloned().unwrap_or(Value::Bool(false)),
        "next_offset": poll.get("next_offset").cloned().unwrap_or(Value::Null),
    })
}

pub fn terminal_process_wait_response(response: TerminalProcessWaitResponse) -> Value {
    let terminal_id = response.terminal_id;
    let output = response.output;
    json!({
        "terminal_id": terminal_id.clone(),
        "process_id": terminal_id,
        "terminal_name": response.terminal_name,
        "status": response.status,
        "wait_status": response.wait_status,
        "busy": response.busy,
        "exited": response.exited,
        "completed": response.completed,
        "timed_out": response.timed_out,
        "finished_by": response.finished_by,
        "exit_code": response.exit_code,
        "timeout_ms": response.timeout_ms,
        "waited_ms": response.waited_ms,
        "output": output.clone(),
        "output_preview": output,
        "output_chars": response.output_chars,
        "truncated": response.truncated,
    })
}

pub fn terminal_result_scope(terminal_count: usize) -> &'static str {
    if terminal_count > 1 {
        "multiple_terminals"
    } else if terminal_count == 0 {
        "no_terminal"
    } else {
        "single_terminal"
    }
}

fn terminal_process_snapshot_map(snapshot: TerminalProcessSnapshot) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert(
        "terminal_id".to_string(),
        Value::String(snapshot.terminal_id.clone()),
    );
    map.insert(
        "process_id".to_string(),
        Value::String(snapshot.terminal_id),
    );
    map.insert(
        "terminal_name".to_string(),
        Value::String(snapshot.terminal_name),
    );
    map.insert("status".to_string(), Value::String(snapshot.status));
    map.insert(
        "process_status".to_string(),
        Value::String(snapshot.process_status),
    );
    map.insert("busy".to_string(), Value::Bool(snapshot.busy));
    map.insert("has_session".to_string(), Value::Bool(true));
    map.insert("command".to_string(), Value::String(snapshot.command));
    map.insert("pid".to_string(), Value::Null);
    map.insert("started_at".to_string(), Value::String(snapshot.started_at));
    map.insert("uptime_seconds".to_string(), Value::Null);
    map.insert("cwd".to_string(), Value::String(snapshot.cwd));
    map.insert("project_id".to_string(), json!(snapshot.project_id));
    map.insert(
        "last_active_at".to_string(),
        Value::String(snapshot.last_active_at),
    );
    map.insert(
        "output_preview".to_string(),
        Value::String(snapshot.output_preview.clone()),
    );
    map.insert(
        "output_tail".to_string(),
        Value::String(snapshot.output_preview),
    );
    map.insert(
        "output_tail_chars".to_string(),
        json!(snapshot.output_tail_chars),
    );
    map.insert("exit_code".to_string(), json!(snapshot.exit_code));
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_scope_tracks_terminal_count() {
        assert_eq!(terminal_result_scope(0), "no_terminal");
        assert_eq!(terminal_result_scope(1), "single_terminal");
        assert_eq!(terminal_result_scope(2), "multiple_terminals");
    }

    #[test]
    fn poll_response_preserves_log_pagination_fields() {
        let value = terminal_process_poll_response(
            TerminalProcessSnapshot {
                terminal_id: "term-1".to_string(),
                terminal_name: "workspace".to_string(),
                status: "running".to_string(),
                process_status: "running".to_string(),
                busy: true,
                command: "cargo check".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                cwd: "/workspace".to_string(),
                project_id: Some("project-1".to_string()),
                last_active_at: "2026-01-01T00:00:01Z".to_string(),
                output_preview: "done".to_string(),
                output_tail_chars: 4,
                exit_code: None,
            },
            TerminalProcessPollDetails {
                offset: Some(3),
                limit: 10,
                has_more: true,
                logs: vec![json!({ "offset": 4, "content": "done" })],
            },
        );

        assert_eq!(value["terminal_id"], "term-1");
        assert_eq!(value["mode"], "offset");
        assert_eq!(value["next_offset"], 5);
        assert_eq!(value["returned_log_count"], 1);
        assert_eq!(value["truncation"]["truncated"], false);
    }
}
