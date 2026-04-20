use std::path::Path;

use portable_pty::{CommandBuilder, SlavePty};

use crate::models::terminal_log::TerminalLog;
use crate::repositories::{terminal_logs, terminals};

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

fn build_default_posix_path(home: &str) -> String {
    let mut segments: Vec<String> = Vec::new();
    if !home.is_empty() {
        segments.push(format!("{home}/.cargo/bin"));
        segments.push(format!("{home}/.local/bin"));
    }
    segments.extend(DEFAULT_POSIX_PATH_SEGMENTS.iter().map(|s| s.to_string()));
    segments.join(":")
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
        let _ = terminals::touch_terminal(terminal_id.as_str()).await;
        let log = TerminalLog::new(terminal_id, "output".to_string(), output);
        let _ = terminal_logs::create_terminal_log(&log).await;
    });
}

pub(super) fn spawn_shell(
    cwd: &Path,
    slave: Box<dyn SlavePty + Send>,
) -> Result<Box<dyn portable_pty::Child + Send + Sync>, String> {
    let shell = select_shell();
    let mut cmd = CommandBuilder::new(shell.clone());
    cmd.env_clear();
    cmd.cwd(cwd);
    cmd.env("PWD", cwd.to_string_lossy().to_string());
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
        if let Ok(path) = std::env::var("PATH") {
            if !path.trim().is_empty() {
                cmd.env("PATH", path);
            }
        }
    } else {
        let home = std::env::var("HOME")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| cwd.to_string_lossy().to_string());
        let path = build_default_posix_path(home.as_str());

        cmd.env("HOME", home.as_str());
        cmd.env("SHELL", shell.as_str());
        cmd.env("PATH", path);

        if let Ok(user) = std::env::var("USER") {
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
        if let Ok(tmpdir) = std::env::var("TMPDIR") {
            if !tmpdir.trim().is_empty() {
                cmd.env("TMPDIR", tmpdir.as_str());
            }
        } else {
            cmd.env("TMPDIR", "/tmp");
        }
    }

    slave
        .spawn_command(cmd)
        .map_err(|e| format!("{shell}: {e}"))
}
