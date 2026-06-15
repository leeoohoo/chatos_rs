use chatos_ai_runtime::TaskBuiltinMcpPromptMode;
use chatos_mcp_runtime::{
    BuiltinMcpPromptLocale, BuiltinMcpServerOptions, BuiltinToolProvider, McpExecutorBuilder,
    builtin_servers_from_kinds, configurable_builtin_kinds, default_runtime_builtin_kinds,
};
use serde_json::Value;

use crate::models::{
    McpCatalogEntry, McpPromptPreviewRequest, McpPromptPreviewResponse, McpUnavailableTool,
    TaskMcpConfig, mcp_builtin_kind_guide,
};

use super::builtin_providers::{build_builtin_registry, build_task_runner_builtin_provider};
use super::workspace_mcp::{resolve_workspace_dir_with_base, selected_builtin_kinds};
use super::{McpCatalogService, TaskService, normalized_optional};

mod catalog;
mod prompt_preview;
