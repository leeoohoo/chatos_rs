mod contracts;
mod crud_handlers;
mod history_handlers;
mod support;
mod ws_handlers;

#[cfg(test)]
mod tests;

use axum::{routing::get, Router};

use self::contracts::{CreateTerminalRequest, TerminalLogQuery, TerminalQuery, WsInput, WsOutput};
use self::crud_handlers::{create_terminal, delete_terminal, get_terminal, list_terminals};
use self::history_handlers::list_terminal_logs;
use self::support::{
    attach_busy, derive_terminal_name, list_terminal_logs_before_page,
    list_terminal_logs_recent_page, normalize_history_before, normalize_history_limit,
    normalize_history_offset,
};
use self::ws_handlers::terminal_ws;

pub(super) const DEFAULT_TERMINAL_HISTORY_LIMIT: i64 = 1200;
pub(super) const MAX_TERMINAL_HISTORY_LIMIT: i64 = 5000;
pub(super) const WS_DEFAULT_SNAPSHOT_LINES: usize = 500;
pub(super) const WS_MAX_SNAPSHOT_LINES: usize = 10_000;

pub fn router() -> Router {
    Router::new()
        .route("/api/terminals", get(list_terminals).post(create_terminal))
        .route(
            "/api/terminals/:id",
            get(get_terminal).delete(delete_terminal),
        )
        .route("/api/terminals/:id/history", get(list_terminal_logs))
        .route("/api/terminals/:id/ws", get(terminal_ws))
}
