// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::SystemMcpKey;
use serde_json::Value;

use crate::system_mcp_descriptor;

#[derive(Debug, Clone, PartialEq)]
pub enum SystemMcpToolCatalog {
    Static(Vec<Value>),
    Dynamic,
}

impl SystemMcpToolCatalog {
    pub fn tools(&self) -> Option<&[Value]> {
        match self {
            Self::Static(tools) => Some(tools.as_slice()),
            Self::Dynamic => None,
        }
    }
}

pub fn system_mcp_tool_catalog(key: SystemMcpKey) -> Result<SystemMcpToolCatalog, String> {
    if let Some(kind) = system_mcp_descriptor(key).embedded_kind {
        return crate::builtin_tool_catalog(kind).map(SystemMcpToolCatalog::Static);
    }
    let tools = match key {
        SystemMcpKey::SandboxImages => crate::sandbox_images::list_tools()
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| "Sandbox Images tool registry returned no tools array".to_string())?,
        SystemMcpKey::ProjectEnvironment => {
            crate::system_tool_catalog::project_environment_tool_definitions()
        }
        SystemMcpKey::ProjectRuntimeEnvironment => {
            crate::system_tool_catalog::project_runtime_environment_info_tool_definitions()
        }
        SystemMcpKey::LocalCommandApproval => {
            crate::system_tool_catalog::local_command_approval_tool_definitions()
        }
        SystemMcpKey::TaskRunnerService => return Ok(SystemMcpToolCatalog::Dynamic),
        _ => {
            return Err(format!(
                "system MCP {} has no tool catalog source",
                key.as_str()
            ))
        }
    };
    Ok(SystemMcpToolCatalog::Static(tools))
}

pub fn system_mcp_static_tools(key: SystemMcpKey) -> Result<Vec<Value>, String> {
    match system_mcp_tool_catalog(key)? {
        SystemMcpToolCatalog::Static(tools) => Ok(tools),
        SystemMcpToolCatalog::Dynamic => Err(format!(
            "system MCP {} uses dynamic tool discovery",
            key.as_str()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_system_mcp_declares_a_static_or_dynamic_catalog() {
        for key in SystemMcpKey::ALL {
            let catalog = system_mcp_tool_catalog(key)
                .unwrap_or_else(|error| panic!("{}: {error}", key.as_str()));
            if let SystemMcpToolCatalog::Static(tools) = catalog {
                assert!(!tools.is_empty(), "{}", key.as_str());
                assert!(tools.iter().all(|tool| tool.get("name").is_some()));
            }
        }
    }
}
