// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod catalog;
pub mod protocol;
pub mod provider;
pub mod service;

pub use catalog::{contains_tool_name, sort_tools_by_name, tool_name, tool_name_set};
pub use protocol::{
    jsonrpc_error, jsonrpc_ok, JsonRpcError, JsonRpcRequest, JsonRpcResponse,
    MCP_ERROR_AUTH_REQUIRED, MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS,
    MCP_ERROR_METHOD_NOT_FOUND, METHOD_INITIALIZE, METHOD_NOTIFICATIONS_INITIALIZED, METHOD_PING,
    METHOD_TOOLS_CALL, METHOD_TOOLS_LIST,
};
pub use provider::{CompositeToolProvider, McpRequestContext, McpToolProvider};
pub use service::{McpJsonRpcService, McpServerInfo};
