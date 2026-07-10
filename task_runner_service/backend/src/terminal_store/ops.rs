// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_builtin_tools::{
    path_with_bundled_tools, terminal_process_list_entry, terminal_process_list_response,
    terminal_process_log_response, terminal_process_poll_response, terminal_process_wait_response,
    terminal_recent_logs_entry, terminal_recent_logs_response, TerminalControllerContext,
    TerminalControllerStore, TerminalProcessPollDetails, TerminalProcessSnapshot,
    TerminalProcessWaitResponse, TerminalRecentLogsEntry,
};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::output::{
    collect_output, collect_output_from_logs, derive_terminal_name, log_value_content, select_logs,
    take_recent_logs,
};
use super::pathing::{canonicalize_existing, display_workspace_path, resolve_target_path};
use super::runtime::{
    append_log, mark_session_exited, refresh_session_status, register_session, session_for_context,
    sessions_for_context, wait_for_session,
};
use super::TaskRunnerTerminalControllerStore;

mod controller_api;
mod session_ops;

fn apply_bundled_tools_path(command: &mut Command) {
    if let Some(path) = path_with_bundled_tools(std::env::var_os("PATH")) {
        command.env("PATH", path);
    }
}

fn build_task_shell_command(
    _context: &TerminalControllerContext,
    _project_root: &Path,
    target_path: &Path,
    shell: &str,
    shell_args: Vec<OsString>,
) -> Result<Command, String> {
    let mut process = Command::new(shell);
    process.args(shell_args);
    process.current_dir(target_path);
    process
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    apply_bundled_tools_path(&mut process);
    Ok(process)
}

fn select_shell() -> String {
    if cfg!(windows) {
        if let Ok(comspec) = std::env::var("COMSPEC") {
            let trimmed = comspec.trim();
            if !trimmed.is_empty() && Path::new(trimmed).exists() {
                return trimmed.to_string();
            }
        }
        return "cmd.exe".to_string();
    }

    if let Ok(shell) = std::env::var("SHELL") {
        let trimmed = shell.trim();
        if !trimmed.is_empty() && Path::new(trimmed).exists() {
            return trimmed.to_string();
        }
    }
    if Path::new("/bin/bash").exists() {
        return "/bin/bash".to_string();
    }
    if Path::new("/bin/zsh").exists() {
        return "/bin/zsh".to_string();
    }
    "/bin/sh".to_string()
}
