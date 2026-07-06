// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use chatos_builtin_tools::path_with_bundled_tools;
use portable_pty::{CommandBuilder, SlavePty};

use crate::models::terminal_log::TerminalLog;
use crate::repositories::{terminal_logs, terminals};
use crate::services::process_isolation;

use super::shell_path::select_shell;

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

fn build_default_posix_path(home: &str) -> std::ffi::OsString {
    let mut segments: Vec<String> = Vec::new();
    if !home.is_empty() {
        segments.push(format!("{home}/.cargo/bin"));
        segments.push(format!("{home}/.local/bin"));
    }
    segments.extend(DEFAULT_POSIX_PATH_SEGMENTS.iter().map(|s| s.to_string()));
    std::ffi::OsString::from(segments.join(":"))
}

fn path_env_with_bundled_tools(base_path: Option<std::ffi::OsString>) -> Option<String> {
    path_with_bundled_tools(base_path).map(|path| path.to_string_lossy().into_owned())
}

fn terminal_home_for(cwd: &Path) -> PathBuf {
    let home = cwd.join(".chatos").join("terminal-home");
    let _ = fs::create_dir_all(home.join(".cache"));
    let _ = fs::create_dir_all(home.join(".local"));
    let _ = fs::create_dir_all(home.join(".cargo"));
    let _ = fs::create_dir_all(home.join(".rustup"));
    let _ = fs::create_dir_all(home.join("tmp"));
    home
}

pub(super) fn spawn_terminal_touch(handle: tokio::runtime::Handle, terminal_id: String) {
    handle.spawn(async move {
        let _ = terminals::touch_terminal(terminal_id.as_str()).await;
    });
}

pub(super) fn spawn_terminal_output_persist(
    handle: tokio::runtime::Handle,
    terminal_id: String,
    output: String,
) {
    handle.spawn(async move {
        if terminals::get_terminal_by_id(terminal_id.as_str())
            .await
            .ok()
            .flatten()
            .is_none()
        {
            return;
        }
        let _ = terminals::touch_terminal(terminal_id.as_str()).await;
        let log = TerminalLog::new(terminal_id, "output".to_string(), output);
        let _ = terminal_logs::create_terminal_log(&log).await;
    });
}

pub(super) fn spawn_shell(
    cwd: &Path,
    slave: Box<dyn SlavePty + Send>,
    user_id: Option<&str>,
) -> Result<Box<dyn portable_pty::Child + Send + Sync>, String> {
    let shell = select_shell();
    let isolation = process_isolation::resolve_for_user(user_id)?;
    let terminal_home = if cfg!(windows) {
        None
    } else {
        Some(terminal_home_for(cwd))
    };
    let fs_view_enabled = process_isolation::filesystem_view_enabled(isolation.as_ref())?;
    let (command, args) = process_isolation::terminal_helper_command(
        shell.as_str(),
        isolation.as_ref(),
        Some(cwd),
        terminal_home.as_deref(),
    )?;
    let mut cmd = CommandBuilder::new(command);
    cmd.args(args);
    cmd.env_clear();
    for (key, value) in process_isolation::helper_env_vars() {
        cmd.env(key, value);
    }
    cmd.cwd(cwd);
    cmd.env(
        "PWD",
        process_isolation::child_cwd_for(isolation.as_ref(), cwd)?,
    );
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    if cfg!(windows) {
        if let Ok(system_root) = std::env::var("SystemRoot") {
            if !system_root.trim().is_empty() {
                cmd.env("SystemRoot", system_root);
            }
        }
        if let Ok(comspec) = std::env::var("COMSPEC") {
            if !comspec.trim().is_empty() {
                cmd.env("COMSPEC", comspec);
            }
        }
        if let Some(path) = path_env_with_bundled_tools(std::env::var_os("PATH")) {
            if !path.trim().is_empty() {
                cmd.env("PATH", path);
            }
        }
    } else {
        let terminal_home = terminal_home
            .as_ref()
            .ok_or_else(|| "missing terminal home".to_string())?;
        process_isolation::prepare_workspace_for_user(terminal_home, isolation.as_ref())?;
        process_isolation::prepare_workspace_for_user(cwd, isolation.as_ref())?;
        let home = process_isolation::child_home_for(isolation.as_ref(), terminal_home)?;
        let base_path = build_default_posix_path(home.as_str());
        let path = if fs_view_enabled {
            base_path.to_string_lossy().into_owned()
        } else {
            path_env_with_bundled_tools(Some(base_path.clone()))
                .unwrap_or_else(|| base_path.to_string_lossy().into_owned())
        };

        cmd.env("HOME", home.as_str());
        cmd.env(
            "XDG_CACHE_HOME",
            Path::new(home.as_str())
                .join(".cache")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "XDG_CONFIG_HOME",
            Path::new(home.as_str())
                .join(".config")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "XDG_DATA_HOME",
            Path::new(home.as_str())
                .join(".local/share")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "npm_config_prefix",
            Path::new(home.as_str())
                .join(".local")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "PIP_CACHE_DIR",
            Path::new(home.as_str())
                .join(".cache/pip")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "CARGO_HOME",
            Path::new(home.as_str())
                .join(".cargo")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "RUSTUP_HOME",
            Path::new(home.as_str())
                .join(".rustup")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "GOMODCACHE",
            Path::new(home.as_str())
                .join(".cache/go/pkg/mod")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "GOCACHE",
            Path::new(home.as_str())
                .join(".cache/go-build")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env("SHELL", shell.as_str());
        cmd.env("PATH", path);

        if let Some(spec) = isolation.as_ref() {
            let user = spec.login_name();
            cmd.env("USER", user.as_str());
            cmd.env("LOGNAME", user.as_str());
        } else if let Ok(user) = std::env::var("USER") {
            if !user.trim().is_empty() {
                cmd.env("USER", user.as_str());
                cmd.env("LOGNAME", user.as_str());
            }
        }
        if let Ok(lang) = std::env::var("LANG") {
            if !lang.trim().is_empty() {
                cmd.env("LANG", lang.as_str());
            }
        } else {
            cmd.env("LANG", "en_US.UTF-8");
        }
        if let Ok(lc_ctype) = std::env::var("LC_CTYPE") {
            if !lc_ctype.trim().is_empty() {
                cmd.env("LC_CTYPE", lc_ctype.as_str());
            }
        }
        let host_tmp = terminal_home.join("tmp");
        cmd.env(
            "TMPDIR",
            process_isolation::child_tmp_for(isolation.as_ref(), host_tmp.as_path())?,
        );
    }

    slave
        .spawn_command(cmd)
        .map_err(|e| format!("{shell}: {e}"))
}
