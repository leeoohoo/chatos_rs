// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod catalog;
pub mod policy;
pub mod protocol;
pub mod provider;
pub mod service;

pub use catalog::{contains_tool_name, sort_tools_by_name, tool_name, tool_name_set};
pub use policy::{
    builtin_kind_header_value, classify_builtin_tool, normalize_builtin_kind_name,
    selected_host_builtin_kind_names, split_builtin_kind_header, BuiltinHostBackend,
    BuiltinToolAccess, HostCapabilityPolicy, BUILTIN_KIND_BROWSER_TOOLS,
    BUILTIN_KIND_CODE_MAINTAINER_READ, BUILTIN_KIND_CODE_MAINTAINER_WRITE,
    BUILTIN_KIND_LOCAL_COMMAND_APPROVAL, BUILTIN_KIND_TERMINAL_CONTROLLER,
    HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
pub use protocol::{
    jsonrpc_error, jsonrpc_ok, CancelledNotificationParams, JsonRpcError, JsonRpcRequest,
    JsonRpcResponse, MCP_ERROR_AUTH_REQUIRED, MCP_ERROR_CAPACITY_EXHAUSTED, MCP_ERROR_INTERNAL,
    MCP_ERROR_INVALID_PARAMS, MCP_ERROR_INVOCATION_CANCELLED, MCP_ERROR_METHOD_NOT_FOUND,
    MCP_ERROR_UNKNOWN_EXECUTION_STATE, METHOD_INITIALIZE, METHOD_NOTIFICATIONS_CANCELLED,
    METHOD_NOTIFICATIONS_INITIALIZED, METHOD_PING, METHOD_TOOLS_CALL, METHOD_TOOLS_LIST,
};
pub use provider::{
    tool_result_max_chars_from_params, CompositeToolProvider, McpRequestContext, McpToolProvider,
    TOOL_RESULT_MAX_CHARS_META_KEY, TOOL_RESULT_MAX_CHARS_UPPER_BOUND,
};
pub use service::{McpJsonRpcService, McpServerInfo};
