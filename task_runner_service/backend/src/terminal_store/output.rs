// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use super::{TerminalLogEntry, TerminalSession};

pub(super) struct OutputCapture {
    pub(super) text: String,
    pub(super) char_count: usize,
    pub(super) truncated: bool,
}

pub(super) async fn collect_output(
    session: &Arc<TerminalSession>,
    max_chars: usize,
) -> OutputCapture {
    let logs = session.logs.lock().await;
    collect_output_from_logs(logs.iter().map(|entry| entry.content.as_str()), max_chars)
}

pub(super) fn collect_output_from_logs<'a, I>(items: I, max_chars: usize) -> OutputCapture
where
    I: Iterator<Item = &'a str>,
{
    let full = items.collect::<Vec<_>>().join("");
    let char_count = full.chars().count();
    if char_count <= max_chars {
        return OutputCapture {
            text: full,
            char_count,
            truncated: false,
        };
    }
    let text = full
        .chars()
        .skip(char_count.saturating_sub(max_chars))
        .collect::<String>();
    OutputCapture {
        text,
        char_count,
        truncated: true,
    }
}

pub(super) fn select_logs(
    logs: &[TerminalLogEntry],
    offset: Option<i64>,
    limit: usize,
) -> Vec<Value> {
    let selected = if let Some(offset) = offset {
        logs.iter()
            .filter(|entry| entry.offset >= offset.max(0))
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        logs.iter().rev().take(limit).collect::<Vec<_>>()
    };
    let ordered = if offset.is_some() {
        selected
    } else {
        selected.into_iter().rev().collect::<Vec<_>>()
    };
    ordered.into_iter().map(log_to_value).collect()
}

pub(super) fn take_recent_logs(logs: &[TerminalLogEntry], limit: usize) -> Vec<Value> {
    logs.iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(log_to_value)
        .collect()
}

fn log_to_value(entry: &TerminalLogEntry) -> Value {
    json!({
        "offset": entry.offset,
        "kind": entry.kind,
        "content": entry.content,
        "created_at": entry.created_at,
    })
}

pub(super) fn log_value_content(value: &Value) -> Option<&str> {
    value.get("content").and_then(Value::as_str)
}

pub(super) fn derive_terminal_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("terminal")
        .to_string()
}
