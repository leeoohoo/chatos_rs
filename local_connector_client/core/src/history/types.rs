// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::relay::RelayRequest;
use crate::{tracing_stdout, LocalState};

const MAX_COMMAND_HISTORY_ENTRIES: usize = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CommandHistoryEntry {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) workspace_id: Option<String>,
    pub(crate) workspace_alias: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    pub(crate) display: String,
    pub(crate) status: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout_preview: Option<String>,
    pub(crate) stderr_preview: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) request_id: Option<String>,
    pub(crate) terminal_session_id: Option<String>,
    pub(crate) sandbox_id: Option<String>,
    pub(crate) tool_name: Option<String>,
}

#[derive(Clone)]
pub(crate) struct CommandHistoryRecorder {
    pub(crate) state_path: PathBuf,
    pub(crate) state: Arc<RwLock<LocalState>>,
}

#[derive(Debug)]
pub(crate) struct CommandExecutionContext {
    pub(crate) source: String,
    pub(crate) request_id: Option<String>,
    pub(crate) tool_name: Option<String>,
    pub(crate) terminal_session_id: Option<String>,
    pub(crate) sandbox_id: Option<String>,
}

impl CommandHistoryRecorder {
    pub(crate) async fn append(&self, entry: CommandHistoryEntry) {
        let mut state = self.state.write().await;
        state.command_history.push(entry);
        let overflow = state
            .command_history
            .len()
            .saturating_sub(MAX_COMMAND_HISTORY_ENTRIES);
        if overflow > 0 {
            state.command_history.drain(0..overflow);
        }
        if let Err(err) = state.save(self.state_path.as_path()) {
            tracing_stdout(format!("save command history failed: {err}").as_str());
        }
    }
}

impl CommandExecutionContext {
    pub(crate) fn terminal_exec(request: &RelayRequest) -> Self {
        Self {
            source: "chatos_terminal_exec".to_string(),
            request_id: Some(request.request_id.clone()),
            tool_name: None,
            terminal_session_id: None,
            sandbox_id: None,
        }
    }

    pub(crate) fn local_mcp(request: &RelayRequest, tool_name: &str) -> Self {
        Self {
            source: "local_mcp".to_string(),
            request_id: Some(request.request_id.clone()),
            tool_name: Some(tool_name.to_string()),
            terminal_session_id: None,
            sandbox_id: None,
        }
    }

    pub(crate) fn task_runner_sandbox(
        request: &RelayRequest,
        sandbox_id: &str,
        tool_name: &str,
    ) -> Self {
        Self {
            source: "task_runner_sandbox".to_string(),
            request_id: Some(request.request_id.clone()),
            tool_name: Some(tool_name.to_string()),
            terminal_session_id: None,
            sandbox_id: Some(sandbox_id.to_string()),
        }
    }
}
