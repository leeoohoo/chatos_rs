// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::registry::LocalMcpTerminalLog;

#[derive(Debug)]
pub(super) struct LocalMcpTerminalOutput {
    pub(super) text: String,
    pub(super) char_count: usize,
    pub(super) truncated: bool,
}

pub(super) fn collect_local_mcp_output_from_logs<'a, I>(
    items: I,
    max_chars: usize,
) -> LocalMcpTerminalOutput
where
    I: Iterator<Item = &'a str>,
{
    let full = items
        .map(strip_local_mcp_internal_shell_markers)
        .collect::<Vec<_>>()
        .join("");
    collect_local_mcp_output_from_text(full, max_chars)
}

pub(super) fn collect_local_mcp_output_from_strings<I>(
    items: I,
    max_chars: usize,
) -> LocalMcpTerminalOutput
where
    I: Iterator<Item = String>,
{
    let full = items.collect::<Vec<_>>().join("");
    collect_local_mcp_output_from_text(full, max_chars)
}

pub(super) fn select_local_mcp_logs(
    logs: &[LocalMcpTerminalLog],
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
    ordered.into_iter().map(local_mcp_log_to_value).collect()
}

pub(super) fn take_recent_local_mcp_logs(logs: &[LocalMcpTerminalLog], limit: usize) -> Vec<Value> {
    logs.iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(local_mcp_log_to_value)
        .collect()
}

pub(super) fn strip_local_mcp_internal_shell_markers(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| {
            !line.contains("__CHATO_LOCAL_CMD_START_") && !line.contains("__CHATO_LOCAL_CMD_DONE_")
        })
        .collect()
}

fn collect_local_mcp_output_from_text(full: String, max_chars: usize) -> LocalMcpTerminalOutput {
    let char_count = full.chars().count();
    if char_count <= max_chars {
        return LocalMcpTerminalOutput {
            text: full,
            char_count,
            truncated: false,
        };
    }
    let text = full
        .chars()
        .skip(char_count.saturating_sub(max_chars))
        .collect::<String>();
    LocalMcpTerminalOutput {
        text,
        char_count,
        truncated: true,
    }
}

fn local_mcp_log_to_value(entry: &LocalMcpTerminalLog) -> Value {
    json!({
        "offset": entry.offset,
        "kind": entry.kind,
        "content": strip_local_mcp_internal_shell_markers(entry.content.as_str()),
        "created_at": entry.created_at,
    })
}
