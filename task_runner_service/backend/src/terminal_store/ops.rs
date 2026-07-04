// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_builtin_tools::{
    path_with_bundled_tools, TerminalControllerContext, TerminalControllerStore,
};
use chatos_mcp_runtime::process_isolation;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::output::{
    collect_output, collect_output_from_logs, derive_terminal_name, log_value_content, select_logs,
    take_recent_logs,
};
use super::pathing::{canonicalize_existing, resolve_target_path};
use super::runtime::{
    append_log, mark_session_exited, refresh_session_status, register_session, session_for_context,
    sessions_for_context, wait_for_session,
};
use super::TaskRunnerTerminalControllerStore;

mod controller_api;
mod session_ops;

const DEFAULT_POSIX_PATH_SEGMENTS: &[&str] = &[
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

fn apply_bundled_tools_path(command: &mut Command) {
    if let Some(path) = path_with_bundled_tools(std::env::var_os("PATH")) {
        command.env("PATH", path);
    }
}

fn build_task_shell_command(
    context: &TerminalControllerContext,
    project_root: &Path,
    target_path: &Path,
    shell: &str,
    shell_args: Vec<OsString>,
) -> Result<Command, String> {
    let isolation = process_isolation::resolve_for_user(context.user_id.as_deref())?;
    let fs_view_enabled = process_isolation::filesystem_view_enabled(isolation.as_ref())?;
    let terminal_home = if cfg!(windows) {
        None
    } else {
        Some(task_terminal_home_for(
            project_root,
            context.user_id.as_deref(),
        ))
    };

    if !cfg!(windows) {
        if let Some(terminal_home) = terminal_home.as_deref() {
            process_isolation::prepare_workspace_for_user(terminal_home, isolation.as_ref())?;
        }
        process_isolation::prepare_workspace_for_user(target_path, isolation.as_ref())?;
    }

    let (command, mut args) = process_isolation::terminal_helper_command(
        shell,
        isolation.as_ref(),
        Some(target_path),
        terminal_home.as_deref(),
    )?;
    args.extend(shell_args);

    let mut process = Command::new(command);
    process.args(args);
    if !fs_view_enabled {
        process.current_dir(target_path);
    }
    process
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if isolation.is_none() {
        apply_bundled_tools_path(&mut process);
        return Ok(process);
    }

    if cfg!(windows) {
        apply_bundled_tools_path(&mut process);
        return Ok(process);
    }

    let terminal_home = terminal_home
        .as_ref()
        .ok_or_else(|| "missing task terminal home".to_string())?;
    let home = process_isolation::child_home_for(isolation.as_ref(), terminal_home)?;
    process.env_clear();
    for (key, value) in process_isolation::helper_env_vars() {
        process.env(key, value);
    }
    process.env(
        "PWD",
        process_isolation::child_cwd_for(isolation.as_ref(), target_path)?,
    );
    process.env("HOME", home.as_str());
    process.env(
        "XDG_CACHE_HOME",
        Path::new(home.as_str())
            .join(".cache")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "XDG_CONFIG_HOME",
        Path::new(home.as_str())
            .join(".config")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "XDG_DATA_HOME",
        Path::new(home.as_str())
            .join(".local/share")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "npm_config_prefix",
        Path::new(home.as_str())
            .join(".local")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "PIP_CACHE_DIR",
        Path::new(home.as_str())
            .join(".cache/pip")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "CARGO_HOME",
        Path::new(home.as_str())
            .join(".cargo")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "RUSTUP_HOME",
        Path::new(home.as_str())
            .join(".rustup")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "GOMODCACHE",
        Path::new(home.as_str())
            .join(".cache/go/pkg/mod")
            .to_string_lossy()
            .to_string(),
    );
    process.env(
        "GOCACHE",
        Path::new(home.as_str())
            .join(".cache/go-build")
            .to_string_lossy()
            .to_string(),
    );
    process.env("SHELL", shell);
    process.env("PATH", build_default_posix_path(home.as_str()));
    process.env("TERM", "xterm-256color");
    process.env("COLORTERM", "truecolor");
    process.env(
        "TMPDIR",
        process_isolation::child_tmp_for(isolation.as_ref(), terminal_home.join("tmp").as_path())?,
    );
    if let Some(spec) = isolation.as_ref() {
        let user = spec.login_name();
        process.env("USER", user.as_str());
        process.env("LOGNAME", user.as_str());
    } else if let Ok(user) = std::env::var("USER") {
        if !user.trim().is_empty() {
            process.env("USER", user.as_str());
            process.env("LOGNAME", user.as_str());
        }
    }
    if let Ok(lang) = std::env::var("LANG") {
        if !lang.trim().is_empty() {
            process.env("LANG", lang.as_str());
        }
    } else {
        process.env("LANG", "en_US.UTF-8");
    }
    Ok(process)
}

fn build_default_posix_path(home: &str) -> OsString {
    let mut segments: Vec<String> = Vec::new();
    if !home.is_empty() {
        segments.push(format!("{home}/.cargo/bin"));
        segments.push(format!("{home}/.local/bin"));
    }
    segments.extend(DEFAULT_POSIX_PATH_SEGMENTS.iter().map(|s| s.to_string()));
    OsString::from(segments.join(":"))
}

fn task_terminal_home_for(project_root: &Path, user_id: Option<&str>) -> PathBuf {
    let home = project_root
        .join(".chatos")
        .join("task-terminal-home")
        .join(normalize_user_segment(user_id.unwrap_or("task_runner")));
    for dir in [
        home.join(".cache"),
        home.join(".local"),
        home.join(".cargo"),
        home.join(".rustup"),
        home.join("tmp"),
    ] {
        let _ = std::fs::create_dir_all(dir);
    }
    home
}

fn normalize_user_segment(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = normalized.trim_matches(['.', '_', '-']);
    if trimmed.is_empty() {
        "user".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
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
