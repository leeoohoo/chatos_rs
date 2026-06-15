use async_trait::async_trait;
use chatos_builtin_tools::{TerminalControllerContext, TerminalControllerStore};
use serde_json::{json, Value};
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
