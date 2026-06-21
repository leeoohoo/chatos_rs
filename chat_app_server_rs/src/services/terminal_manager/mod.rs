mod directory_guard;
mod io_runtime;
mod manager;
mod output_history;
mod path_utils;
mod prompt_parser;
mod session;
mod shell_path;

#[cfg(test)]
mod tests;

pub use self::manager::{get_terminal_manager, TerminalsManager};
pub use self::session::TerminalSession;

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Output(String),
    Exit(i32),
    State(bool),
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn input_triggers_busy(data: &str) -> bool {
    if data.is_empty() {
        return false;
    }
    if data.contains('\r') || data.contains('\n') {
        return true;
    }

    // Ctrl-C / Ctrl-D / Ctrl-Z may start or interrupt foreground commands.
    data.as_bytes()
        .iter()
        .any(|b| matches!(*b, 0x03 | 0x04 | 0x1A))
}
