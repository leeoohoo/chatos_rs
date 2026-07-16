// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_terminal_runtime::TerminalLogBuffer;
use serde_json::Value;

use super::TerminalSession;

pub(super) use chatos_terminal_runtime::{
    collect_output_from_texts as collect_output_from_logs, derive_terminal_name, log_value_content,
    OutputCapture,
};

pub(super) async fn collect_output(
    session: &Arc<TerminalSession>,
    max_chars: usize,
) -> OutputCapture {
    let logs = session.logs.lock().await;
    logs.capture(max_chars)
}

pub(super) fn select_logs(
    logs: &TerminalLogBuffer,
    offset: Option<i64>,
    limit: usize,
) -> Vec<Value> {
    logs.select_json(offset, limit)
}

pub(super) fn take_recent_logs(logs: &TerminalLogBuffer, limit: usize) -> Vec<Value> {
    logs.recent_json(limit)
}
