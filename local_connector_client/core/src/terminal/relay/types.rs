// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionCreateRequest {
    pub(super) terminal_session_id: String,
    pub(super) cwd: Option<String>,
    pub(super) cols: Option<u16>,
    pub(super) rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionInputRequest {
    pub(super) terminal_session_id: String,
    pub(super) data: String,
    pub(super) command: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionCommandRequest {
    pub(super) terminal_session_id: String,
    pub(super) command: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionResizeRequest {
    pub(super) terminal_session_id: String,
    pub(super) cols: u16,
    pub(super) rows: u16,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionSnapshotRequest {
    pub(super) terminal_session_id: String,
    pub(super) lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalSessionCloseRequest {
    pub(super) terminal_session_id: String,
}
