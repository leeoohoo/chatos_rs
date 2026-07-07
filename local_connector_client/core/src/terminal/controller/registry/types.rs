// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

#[derive(Default)]
pub(in crate::terminal::controller) struct LocalMcpTerminalRegistry {
    pub(in crate::terminal::controller) sessions:
        RwLock<HashMap<String, Arc<LocalMcpTerminalSession>>>,
}

pub(in crate::terminal::controller) struct LocalMcpTerminalSession {
    pub(in crate::terminal::controller) meta: Mutex<LocalMcpTerminalMeta>,
    pub(in crate::terminal::controller) child: Mutex<tokio::process::Child>,
    pub(in crate::terminal::controller) stdin: Mutex<Option<tokio::process::ChildStdin>>,
    pub(in crate::terminal::controller) logs: Mutex<Vec<LocalMcpTerminalLog>>,
    pub(in crate::terminal::controller) command_lock: Mutex<()>,
    pub(in crate::terminal::controller) active_shell_marker: Mutex<Option<String>>,
}

#[derive(Debug, Clone)]
pub(in crate::terminal::controller) struct LocalMcpTerminalMeta {
    pub(in crate::terminal::controller) id: String,
    pub(in crate::terminal::controller) root: String,
    pub(in crate::terminal::controller) cwd: String,
    pub(in crate::terminal::controller) project_id: Option<String>,
    pub(in crate::terminal::controller) user_id: Option<String>,
    pub(in crate::terminal::controller) command: String,
    pub(in crate::terminal::controller) started_at: String,
    pub(in crate::terminal::controller) last_active_at: String,
    pub(in crate::terminal::controller) finished_at: Option<String>,
    pub(in crate::terminal::controller) status: String,
    pub(in crate::terminal::controller) exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub(in crate::terminal::controller) struct LocalMcpTerminalLog {
    pub(in crate::terminal::controller) offset: i64,
    pub(in crate::terminal::controller) kind: String,
    pub(in crate::terminal::controller) content: String,
    pub(in crate::terminal::controller) created_at: String,
}

#[derive(Debug)]
pub(in crate::terminal::controller) struct LocalMcpTerminalWaitResult {
    pub(in crate::terminal::controller) waited_ms: u64,
    pub(in crate::terminal::controller) busy: bool,
    pub(in crate::terminal::controller) timed_out: bool,
    pub(in crate::terminal::controller) finished_by: &'static str,
    pub(in crate::terminal::controller) exit_code: Option<i32>,
}
