#[cfg(test)]
#[path = "builtin_mcp_prompt/tests.rs"]
mod tests;

use std::collections::HashMap;

use serde_json::Value;

use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_tools::ToolInfo;
use crate::services::mcp_loader::McpBuiltinServer;

pub use chatos_mcp_runtime::BuiltinMcpPromptBuildResult;

pub fn builtin_mcp_prompt_source_path(locale: InternalContextLocale) -> &'static str {
    chatos_mcp_runtime::builtin_mcp_prompt_source_path(shared_prompt_locale(locale))
}

pub fn builtin_mcp_prompt_section_ids(locale: InternalContextLocale) -> Vec<String> {
    chatos_mcp_runtime::builtin_mcp_prompt_section_ids(shared_prompt_locale(locale))
}

pub fn compose_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    locale: InternalContextLocale,
) -> Option<String> {
    chatos_mcp_runtime::compose_builtin_mcp_system_prompt(
        shared_builtin_servers(builtin_servers).as_slice(),
        shared_prompt_locale(locale),
    )
}

pub fn inspect_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    locale: InternalContextLocale,
) -> BuiltinMcpPromptBuildResult {
    chatos_mcp_runtime::inspect_builtin_mcp_system_prompt(
        shared_builtin_servers(builtin_servers).as_slice(),
        shared_prompt_locale(locale),
    )
}

pub fn compose_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    locale: InternalContextLocale,
) -> Option<String> {
    chatos_mcp_runtime::compose_effective_builtin_mcp_system_prompt(
        shared_builtin_servers(builtin_servers).as_slice(),
        &shared_tool_metadata(tool_metadata),
        unavailable_tools,
        shared_prompt_locale(locale),
    )
}

pub fn inspect_effective_builtin_mcp_system_prompt(
    builtin_servers: &[McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    unavailable_tools: &[Value],
    locale: InternalContextLocale,
) -> BuiltinMcpPromptBuildResult {
    chatos_mcp_runtime::inspect_effective_builtin_mcp_system_prompt(
        shared_builtin_servers(builtin_servers).as_slice(),
        &shared_tool_metadata(tool_metadata),
        unavailable_tools,
        shared_prompt_locale(locale),
    )
}

fn shared_prompt_locale(
    locale: InternalContextLocale,
) -> chatos_mcp_runtime::BuiltinMcpPromptLocale {
    if locale.is_english() {
        chatos_mcp_runtime::BuiltinMcpPromptLocale::EnUs
    } else {
        chatos_mcp_runtime::BuiltinMcpPromptLocale::ZhCn
    }
}

fn shared_builtin_servers(
    builtin_servers: &[McpBuiltinServer],
) -> Vec<chatos_mcp_runtime::McpBuiltinServer> {
    builtin_servers
        .iter()
        .cloned()
        .map(crate::services::shared_mcp_runtime::shared_builtin_server)
        .collect()
}

fn shared_tool_metadata(
    tool_metadata: &HashMap<String, ToolInfo>,
) -> HashMap<String, chatos_mcp_runtime::ToolInfo> {
    tool_metadata
        .iter()
        .map(|(name, info)| (name.clone(), shared_tool_info(info)))
        .collect()
}

fn shared_tool_info(info: &ToolInfo) -> chatos_mcp_runtime::ToolInfo {
    chatos_mcp_runtime::ToolInfo {
        original_name: info.original_name.clone(),
        server_name: info.server_name.clone(),
        server_type: info.server_type.clone(),
        server_url: info.server_url.clone(),
        server_config: info.server_config.clone().map(|server| {
            chatos_mcp_runtime::McpStdioServer {
                name: server.name,
                command: server.command,
                args: server.args,
                cwd: server.cwd,
                env: server.env,
            }
        }),
        tool_info: info.tool_info.clone(),
    }
}
