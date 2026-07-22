// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use crate::models::mcp_config::McpConfig;

pub use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_mcp_runtime::{LEGACY_CODE_MAINTAINER_COMMAND, LEGACY_CODE_MAINTAINER_MCP_ID};

pub fn builtin_kind_by_id(id: &str) -> Option<BuiltinMcpKind> {
    chatos_mcp::system_mcp_descriptor_by_any(id).and_then(|descriptor| descriptor.embedded_kind)
}

pub fn builtin_kind_by_command(command: &str) -> Option<BuiltinMcpKind> {
    chatos_mcp_runtime::builtin_kind_by_command(command)
}

pub fn is_builtin_mcp_id(id: &str) -> bool {
    chatos_mcp::system_mcp_descriptor_by_resource_id(id)
        .is_some_and(|descriptor| descriptor.embedded_kind.is_some())
        || id == LEGACY_CODE_MAINTAINER_MCP_ID
}

pub fn get_builtin_mcp_config(id: &str) -> Option<McpConfig> {
    if id == LEGACY_CODE_MAINTAINER_MCP_ID {
        return builtin_config(
            BuiltinMcpKind::CodeMaintainerWrite,
            LEGACY_CODE_MAINTAINER_MCP_ID,
            LEGACY_CODE_MAINTAINER_COMMAND,
        );
    }
    let descriptor = chatos_mcp::system_mcp_descriptor_by_resource_id(id)?;
    if !descriptor.supports_host(chatos_mcp::SystemMcpHost::Chatos) {
        return None;
    }
    let kind = descriptor.embedded_kind?;
    builtin_config(kind, descriptor.resource_id, kind.command()?)
}

pub fn list_builtin_mcp_configs() -> Vec<McpConfig> {
    chatos_mcp::system_mcp_catalog()
        .iter()
        .filter(|descriptor| descriptor.supports_host(chatos_mcp::SystemMcpHost::Chatos))
        .filter_map(|descriptor| {
            let kind = descriptor.embedded_kind?;
            builtin_config(kind, descriptor.resource_id, kind.command()?)
        })
        .collect()
}

pub fn builtin_display_name(id: &str) -> Option<&'static str> {
    if id == LEGACY_CODE_MAINTAINER_MCP_ID {
        return Some(
            chatos_mcp::system_mcp_descriptor(
                chatos_plugin_management_sdk::SystemMcpKey::CodeMaintainerWrite,
            )
            .display_name,
        );
    }
    chatos_mcp::system_mcp_descriptor_by_resource_id(id)
        .filter(|descriptor| descriptor.embedded_kind.is_some())
        .map(|descriptor| descriptor.display_name)
}

fn builtin_config(kind: BuiltinMcpKind, id: &str, command: &str) -> Option<McpConfig> {
    let now = crate::core::time::now_rfc3339();
    Some(McpConfig {
        id: id.to_string(),
        name: kind.server_name().to_string(),
        command: command.to_string(),
        r#type: "stdio".to_string(),
        args: Some(json!(["--name", kind.server_name()])),
        env: None,
        cwd: None,
        user_id: None,
        enabled: true,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_runtime::{
        BROWSER_TOOLS_COMMAND, BROWSER_TOOLS_MCP_ID, WEB_TOOLS_COMMAND, WEB_TOOLS_MCP_ID,
    };

    #[test]
    fn web_and_browser_builtin_are_registered() {
        assert_eq!(
            builtin_kind_by_id(WEB_TOOLS_MCP_ID),
            Some(BuiltinMcpKind::WebTools)
        );
        assert_eq!(
            builtin_kind_by_command(WEB_TOOLS_COMMAND),
            Some(BuiltinMcpKind::WebTools)
        );
        assert_eq!(
            builtin_kind_by_id(BROWSER_TOOLS_MCP_ID),
            Some(BuiltinMcpKind::BrowserTools)
        );
        assert_eq!(
            builtin_kind_by_command(BROWSER_TOOLS_COMMAND),
            Some(BuiltinMcpKind::BrowserTools)
        );
    }

    #[test]
    fn builtin_mcp_config_list_contains_web_and_browser() {
        let ids: Vec<String> = list_builtin_mcp_configs()
            .into_iter()
            .map(|cfg| cfg.id)
            .collect();
        assert!(ids.contains(&WEB_TOOLS_MCP_ID.to_string()));
        assert!(ids.contains(&BROWSER_TOOLS_MCP_ID.to_string()));
    }
}
