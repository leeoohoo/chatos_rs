// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod builder;
pub mod builtin_catalog;
pub mod builtin_prompt;
pub mod executor;
pub mod naming;
pub mod parallelism;
pub mod registry;
pub mod rpc;
pub mod schema;
pub mod text;
pub mod tool_call;
pub mod types;

pub use builder::McpExecutorBuilder;
pub use builtin_catalog::{
    builtin_kind_by_any, builtin_kind_by_command, builtin_kind_by_config_id,
    builtin_kind_by_kind_name, builtin_kind_by_server_name, builtin_servers_from_kinds,
    complete_builtin_kind_dependencies, configurable_builtin_kinds, default_runtime_builtin_kinds,
    BuiltinMcpKind, BuiltinMcpServerOptions, AGENT_BUILDER_COMMAND, AGENT_BUILDER_MCP_ID,
    AGENT_BUILDER_SERVER_NAME, ASK_USER_COMMAND, ASK_USER_MCP_ID, ASK_USER_SERVER_NAME,
    BROWSER_TOOLS_COMMAND, BROWSER_TOOLS_MCP_ID, BROWSER_TOOLS_SERVER_NAME,
    CODE_MAINTAINER_READ_COMMAND, CODE_MAINTAINER_READ_MCP_ID, CODE_MAINTAINER_READ_SERVER_NAME,
    CODE_MAINTAINER_WRITE_COMMAND, CODE_MAINTAINER_WRITE_MCP_ID, CODE_MAINTAINER_WRITE_SERVER_NAME,
    DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_WRITE_BYTES, DEFAULT_SEARCH_LIMIT,
    LEGACY_CODE_MAINTAINER_COMMAND, LEGACY_CODE_MAINTAINER_MCP_ID,
    MEMORY_COMMAND_READER_SERVER_NAME, MEMORY_PLUGIN_READER_SERVER_NAME,
    MEMORY_SKILL_READER_SERVER_NAME, NOTEPAD_COMMAND, NOTEPAD_MCP_ID, NOTEPAD_SERVER_NAME,
    PROJECT_MANAGEMENT_COMMAND, PROJECT_MANAGEMENT_MCP_ID, PROJECT_MANAGEMENT_SERVER_NAME,
    REMOTE_CONNECTION_CONTROLLER_COMMAND, REMOTE_CONNECTION_CONTROLLER_MCP_ID,
    REMOTE_CONNECTION_CONTROLLER_SERVER_NAME, TASK_MANAGER_COMMAND, TASK_MANAGER_MCP_ID,
    TASK_MANAGER_SERVER_NAME, TERMINAL_CONTROLLER_COMMAND, TERMINAL_CONTROLLER_MCP_ID,
    TERMINAL_CONTROLLER_SERVER_NAME, WEB_TOOLS_COMMAND, WEB_TOOLS_MCP_ID, WEB_TOOLS_SERVER_NAME,
};
pub use builtin_prompt::{
    builtin_mcp_prompt_section_ids, builtin_mcp_prompt_source_path,
    compose_builtin_mcp_system_prompt, compose_effective_builtin_mcp_system_prompt,
    inspect_builtin_mcp_system_prompt, inspect_effective_builtin_mcp_system_prompt,
    BuiltinMcpPromptBuildResult, BuiltinMcpPromptLocale,
};
pub use executor::McpExecutor;
pub use naming::{canonical_name_segment, canonical_prefixed_tool_name, legacy_prefixed_tool_name};
pub use registry::{BuiltinToolProvider, BuiltinToolRegistry};
pub use rpc::{jsonrpc_http_call, jsonrpc_stdio_call, list_tools_http, list_tools_stdio};
pub use schema::{build_function_tool_schema, parse_tool_definition};
pub use text::{inject_agent_builder_args, to_text_and_structured_result};
pub use types::{
    McpBuiltinServer, McpHttpServer, McpStdioServer, ParsedToolDefinition, ToolAbortCheckCallback,
    ToolCallContext, ToolCallerModelRuntime, ToolInfo, ToolResult, ToolResultCallback,
    ToolStreamChunkCallback,
};
