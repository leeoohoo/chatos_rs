// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "mcp_execution_core/execution.rs"]
mod execution;
#[path = "mcp_execution_core/executor.rs"]
mod executor;
#[path = "mcp_execution_core/lifecycle.rs"]
mod lifecycle;
#[path = "mcp_execution_core/registration.rs"]
mod registration;
#[path = "mcp_execution_core/state.rs"]
mod state;

pub(crate) use self::execution::{execute_tools_stream_with_registry, response_tool_name};
pub(crate) use self::executor::McpExecutorCore;
pub(crate) use self::lifecycle::build_builtin_tool_state;
pub(crate) use self::registration::{codex_gateway_request_tools, register_tools_from_builtin};
pub(crate) use self::state::McpToolState;

#[cfg(test)]
use self::execution::is_heavy_io_tool_name;
#[cfg(test)]
mod tests {
    use super::is_heavy_io_tool_name;

    #[test]
    fn heavy_io_tool_policy_covers_workspace_file_operations() {
        for name in [
            "read_file",
            "read_file_range",
            "read_file_raw",
            "search_text",
            "list_dir",
            "write_file",
            "edit_file",
            "apply_patch",
        ] {
            assert!(is_heavy_io_tool_name(name), "{name} should be IO limited");
        }
        assert!(!is_heavy_io_tool_name("get_recent_logs"));
        assert!(!is_heavy_io_tool_name("web_search"));
    }
}
