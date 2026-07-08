// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::OnceLock;

mod lifecycle;
mod logs;
mod query;
mod types;

pub(super) use lifecycle::{
    mark_local_mcp_terminal_exited, refresh_local_mcp_terminal_session_status,
    register_local_mcp_terminal_session,
};
pub(super) use logs::{
    append_local_mcp_terminal_log, collect_local_mcp_terminal_output,
    collect_local_mcp_terminal_output_by_kinds, collect_local_mcp_terminal_output_since_by_kinds,
    next_local_mcp_log_offset,
};
pub(super) use query::{
    local_mcp_session_for_context, local_mcp_sessions_for_context,
    wait_for_local_mcp_terminal_session,
};
pub(super) use types::{
    LocalMcpTerminalLog, LocalMcpTerminalRegistry, LocalMcpTerminalSession,
    LocalMcpTerminalWaitResult,
};

pub(super) fn local_mcp_terminal_registry() -> &'static LocalMcpTerminalRegistry {
    static REGISTRY: OnceLock<LocalMcpTerminalRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LocalMcpTerminalRegistry::default)
}
