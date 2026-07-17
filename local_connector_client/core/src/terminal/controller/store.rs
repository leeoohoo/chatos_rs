// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_builtin_tools::{TerminalControllerContext, TerminalControllerStore};
use serde_json::Value;

use super::shell::{
    canonicalize_terminal_root, display_local_mcp_workspace_path, resolve_terminal_controller_cwd,
};
use super::{
    execute_local_mcp_reused_shell_command, execute_local_mcp_standalone_command,
    local_mcp_shell_session_is_busy, LocalConnectorTerminalControllerStore,
};

mod logs;
mod process;

#[async_trait::async_trait]
impl TerminalControllerStore for LocalConnectorTerminalControllerStore {
    async fn execute_command(
        &self,
        context: TerminalControllerContext,
        path: String,
        command: String,
        background: bool,
        permissions: chatos_builtin_tools::TerminalCommandPermissions,
    ) -> std::result::Result<Value, String> {
        if !permissions.is_empty() {
            return Err(
                "temporary permission overlays require an active sandbox lease".to_string(),
            );
        }
        let project_root = canonicalize_terminal_root(context.root.as_path())?;
        let cwd = resolve_terminal_controller_cwd(project_root.as_path(), path.as_str())?;
        let display_project_root =
            display_local_mcp_workspace_path(project_root.as_path(), project_root.as_path());
        let display_cwd = display_local_mcp_workspace_path(project_root.as_path(), cwd.as_path());

        if background || cfg!(windows) {
            return execute_local_mcp_standalone_command(
                context,
                project_root,
                cwd,
                display_project_root,
                display_cwd,
                command,
                background,
                if background {
                    Some("background")
                } else {
                    Some("windows")
                },
            )
            .await;
        }

        let session = self
            .ensure_shell_session(context.clone(), path.clone())
            .await?;
        if local_mcp_shell_session_is_busy(&session).await {
            return execute_local_mcp_standalone_command(
                context,
                project_root,
                cwd,
                display_project_root,
                display_cwd,
                command,
                false,
                Some("primary_terminal_busy"),
            )
            .await;
        }

        execute_local_mcp_reused_shell_command(
            context,
            session,
            project_root,
            cwd,
            display_project_root,
            display_cwd,
            command,
        )
        .await
    }

    async fn get_recent_logs(
        &self,
        context: TerminalControllerContext,
        per_terminal_limit: i64,
        terminal_limit: usize,
    ) -> std::result::Result<Value, String> {
        logs::get_recent_logs(context, per_terminal_limit, terminal_limit).await
    }

    async fn process_list(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> std::result::Result<Value, String> {
        process::process_list(context, include_exited, limit).await
    }

    async fn process_poll(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> std::result::Result<Value, String> {
        process::process_poll(context, terminal_id, offset, limit).await
    }

    async fn process_log(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> std::result::Result<Value, String> {
        process::process_log(context, terminal_id, offset, limit).await
    }

    async fn process_wait(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        timeout_ms: u64,
    ) -> std::result::Result<Value, String> {
        process::process_wait(context, terminal_id, timeout_ms).await
    }

    async fn process_write(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        data: String,
        submit: bool,
    ) -> std::result::Result<Value, String> {
        process::process_write(context, terminal_id, data, submit).await
    }

    async fn process_kill(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
    ) -> std::result::Result<Value, String> {
        process::process_kill(context, terminal_id).await
    }
}
