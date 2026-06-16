use chatos_ai_runtime::TaskBuiltinMcpPromptMode;
use chatos_mcp_runtime::{
    builtin_servers_from_kinds, configurable_builtin_kinds, default_runtime_builtin_kinds,
    BuiltinMcpPromptLocale, BuiltinMcpServerOptions, BuiltinToolProvider, McpExecutorBuilder,
};
use serde_json::Value;

use crate::models::{
    mcp_builtin_kind_guide, McpCatalogEntry, McpPromptPreviewRequest, McpPromptPreviewResponse,
    McpUnavailableTool, TaskMcpConfig,
};

use super::builtin_providers::{build_builtin_registry, build_task_runner_builtin_provider};
use super::workspace_mcp::{resolve_workspace_dir_with_base, selected_builtin_kinds};
use super::{normalized_optional, McpCatalogService, TaskService};

mod catalog;
mod prompt_preview;
