// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::system_mcp_catalog::{
    system_mcp_descriptor_for_record, system_mcp_provider_skills, system_mcp_tool_catalog,
    SystemMcpToolCatalog,
};
use serde_json::Value;

use crate::config::AppConfig;
use crate::models::{McpProviderSkill, McpRecord, RUNTIME_KIND_HTTP};

#[derive(Debug, Default)]
pub(crate) struct LiveMcpDescriptor {
    pub skills: Vec<McpProviderSkill>,
    pub tools: Vec<Value>,
}

pub(crate) async fn live_mcp_descriptor(
    _config: &AppConfig,
    record: &McpRecord,
) -> Result<Option<LiveMcpDescriptor>, String> {
    if system_mcp_descriptor_for_record(record).is_some() {
        return live_system_mcp_descriptor(record).await.map(Some);
    }
    match record.runtime.kind.as_str() {
        // External MCP runtimes are executed and inspected only by the Local
        // Connector Client. Plugin Management may return a previously synced
        // tool snapshot, but it must never connect to the MCP endpoint itself.
        RUNTIME_KIND_HTTP => Ok(None),
        _ => Ok(None),
    }
}

async fn live_system_mcp_descriptor(record: &McpRecord) -> Result<LiveMcpDescriptor, String> {
    let descriptor = system_mcp_descriptor_for_record(record)
        .ok_or_else(|| format!("unknown system MCP: {}", record.id))?;
    let tools = match system_mcp_tool_catalog(descriptor.key)? {
        SystemMcpToolCatalog::Static(tools) => tools,
        SystemMcpToolCatalog::Dynamic => Vec::new(),
    };
    let skills = system_mcp_provider_skills(descriptor.key)
        .into_iter()
        .map(|skill| serde_json::from_value(serde_json::to_value(skill).unwrap_or(Value::Null)))
        .collect::<Result<Vec<McpProviderSkill>, _>>()
        .map_err(|error| format!("decode system MCP provider skills failed: {error}"))?;
    Ok(LiveMcpDescriptor { skills, tools })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_static_system_mcp_has_real_tools() {
        for descriptor in crate::system_mcp_catalog::system_mcp_catalog() {
            let catalog = system_mcp_tool_catalog(descriptor.key).expect("catalog");
            if let SystemMcpToolCatalog::Static(tools) = catalog {
                assert!(!tools.is_empty(), "{}", descriptor.server_name);
                assert!(tools.iter().all(|tool| tool.get("name").is_some()));
            }
        }
    }
}
