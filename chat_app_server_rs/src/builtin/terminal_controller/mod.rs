mod actions;
mod capture;
mod context;

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::Value;

use chatos_builtin_tools::{TerminalControllerContext, TerminalControllerStore};

use self::actions::actions_execute::execute_command_with_context;
use self::actions::actions_process::{
    kill_process_with_context, poll_process_with_context, read_process_log_with_context,
    wait_process_with_context, write_process_with_context,
};
use self::actions::actions_query::{get_recent_logs_with_context, list_processes_with_context};

pub(super) const RECENT_LOGS_PER_ENTRY_MAX_CHARS: usize = 1_500;
pub(super) const RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL: usize = 16_000;

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) root: PathBuf,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) idle_timeout_ms: u64,
    pub(super) max_wait_ms: u64,
    pub(super) max_output_chars: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ChatosTerminalControllerStore;

#[async_trait]
impl TerminalControllerStore for ChatosTerminalControllerStore {
    async fn execute_command(
        &self,
        context: TerminalControllerContext,
        path: String,
        command: String,
        background: bool,
    ) -> Result<Value, String> {
        execute_command_with_context(
            bound_context(context),
            path.as_str(),
            command.as_str(),
            background,
        )
        .await
    }

    async fn get_recent_logs(
        &self,
        context: TerminalControllerContext,
        per_terminal_limit: i64,
        terminal_limit: usize,
    ) -> Result<Value, String> {
        get_recent_logs_with_context(bound_context(context), per_terminal_limit, terminal_limit)
            .await
    }

    async fn process_list(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String> {
        list_processes_with_context(bound_context(context), include_exited, limit).await
    }

    async fn process_poll(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        poll_process_with_context(bound_context(context), terminal_id.as_str(), offset, limit).await
    }

    async fn process_log(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        read_process_log_with_context(bound_context(context), terminal_id.as_str(), offset, limit)
            .await
    }

    async fn process_wait(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        timeout_ms: u64,
    ) -> Result<Value, String> {
        wait_process_with_context(bound_context(context), terminal_id.as_str(), timeout_ms).await
    }

    async fn process_write(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        data: String,
        submit: bool,
    ) -> Result<Value, String> {
        write_process_with_context(
            bound_context(context),
            terminal_id.as_str(),
            data.as_str(),
            submit,
        )
        .await
    }

    async fn process_kill(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
    ) -> Result<Value, String> {
        kill_process_with_context(bound_context(context), terminal_id.as_str()).await
    }
}

fn bound_context(context: TerminalControllerContext) -> BoundContext {
    BoundContext {
        root: context.root,
        user_id: context.user_id,
        project_id: context.project_id,
        idle_timeout_ms: context.idle_timeout_ms,
        max_wait_ms: context.max_wait_ms,
        max_output_chars: context.max_output_chars,
    }
}
