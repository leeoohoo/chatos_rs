// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_mcp::{
    configure_child_process_group as configure_task_terminal_process_group,
    path_with_bundled_tools, terminal_process_list_entry, terminal_process_list_response,
    terminal_process_log_response, terminal_process_poll_response, terminal_process_wait_response,
    terminal_recent_logs_entry, terminal_recent_logs_response,
    terminate_child_process_tree as terminate_task_terminal_process_tree,
    TerminalControllerContext, TerminalControllerStore, TerminalProcessPollDetails,
    TerminalProcessSnapshot, TerminalProcessWaitResponse, TerminalRecentLogsEntry,
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
    configure_task_terminal_process_group(&mut process);
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

#[cfg(all(test, unix))]
mod tests {
    use std::fs;

    use super::*;

    #[tokio::test]
    async fn task_terminal_kill_terminates_nested_process_group() {
        let root = std::env::temp_dir().join(format!(
            "chatos-task-terminal-process-tree-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.as_path()).expect("create task terminal root");
        let context = TerminalControllerContext {
            root: root.clone(),
            user_id: Some("user-1".to_string()),
            project_id: Some("project-1".to_string()),
            idle_timeout_ms: 1_000,
            max_wait_ms: 30_000,
            max_output_chars: 32_000,
        };
        let mut command = build_task_shell_command(
            &context,
            root.as_path(),
            root.as_path(),
            "/bin/sh",
            vec![
                OsString::from("-lc"),
                OsString::from(r#"sleep 30 & child=$!; printf "%s" "$child" > child.pid; wait"#),
            ],
        )
        .expect("build task shell command");
        let mut child = command.spawn().expect("spawn nested task process");
        let child_pid_path = root.join("child.pid");
        for _ in 0..100 {
            if child_pid_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let child_pid = fs::read_to_string(child_pid_path.as_path())
            .expect("nested task child pid")
            .trim()
            .parse::<i32>()
            .expect("parse nested task child pid");
        assert!(process_exists(child_pid));

        terminate_task_terminal_process_tree(&mut child)
            .await
            .expect("terminate task process tree");
        for _ in 0..100 {
            if !process_exists(child_pid) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(!process_exists(child_pid));
        fs::remove_dir_all(root.as_path()).expect("cleanup task terminal root");
    }

    fn process_exists(pid: i32) -> bool {
        if unsafe { libc::kill(pid, 0) } == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
}
