use std::path::Path;

use portable_pty::{CommandBuilder, SlavePty};

use crate::models::terminal_log::TerminalLog;
use crate::repositories::{terminal_logs, terminals};

use super::shell_path::select_shell;

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
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    slave
        .spawn_command(cmd)
        .map_err(|e| format!("{shell}: {e}"))
}
