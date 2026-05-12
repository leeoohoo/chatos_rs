#[path = "builtin_mcp_prompt/availability.rs"]
mod availability;
#[path = "builtin_mcp_prompt/compose.rs"]
mod compose;
#[path = "builtin_mcp_prompt/sections.rs"]
mod sections;
#[cfg(test)]
#[path = "builtin_mcp_prompt/tests.rs"]
mod tests;

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_tools::ToolInfo;
use crate::services::mcp_loader::McpBuiltinServer;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BuiltinMcpPromptBuildResult {
    pub prompt: Option<String>,
    pub selected_section_ids: Vec<String>,
    pub omitted_section_ids: Vec<String>,
    pub requested_builtin_server_names: Vec<String>,
    pub active_builtin_server_names: Vec<String>,
    pub omitted_builtin_server_names: Vec<String>,
    pub runtime_limitations: Option<String>,
}

pub fn builtin_mcp_prompt_source_path(locale: InternalContextLocale) -> &'static str {
    sections::prompt_source_path(locale)
}

pub fn builtin_mcp_prompt_section_ids(locale: InternalContextLocale) -> Vec<String> {
    sections::prompt_section_registry(locale).ordered_ids.clone()
}

pub fn compose_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    locale: InternalContextLocale,
) -> Option<String> {
    inspect_builtin_mcp_system_prompt(builtin_servers, locale).prompt
}

pub fn inspect_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    locale: InternalContextLocale,
) -> BuiltinMcpPromptBuildResult {
    compose::inspect_builtin_prompt(builtin_servers, sections::prompt_section_registry(locale))
}

pub fn compose_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    locale: InternalContextLocale,
) -> Option<String> {
    inspect_effective_builtin_mcp_system_prompt(
        builtin_servers,
        tool_metadata,
        unavailable_tools,
        locale,
    )
    .prompt
}

pub fn inspect_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    locale: InternalContextLocale,
) -> BuiltinMcpPromptBuildResult {
    availability::inspect_effective_prompt(
        builtin_servers,
        tool_metadata,
        unavailable_tools,
        sections::prompt_section_registry(locale),
    )
}
