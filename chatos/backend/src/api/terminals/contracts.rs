// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct TerminalQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateTerminalRequest {
    pub(super) name: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TerminalLogQuery {
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
    pub(super) before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DispatchTerminalCommandRequest {
    pub(super) cwd: Option<String>,
    pub(super) command: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) create_if_missing: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct InterruptTerminalRequest {
    pub(super) reason: Option<String>,
}
