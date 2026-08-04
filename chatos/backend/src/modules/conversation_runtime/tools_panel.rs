// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeMcpServerCounts {
    pub http: usize,
    pub stdio: usize,
    pub builtin: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeMcpToolsPanel {
    pub tools: Vec<Value>,
    pub count: usize,
    pub unavailable_tools: Vec<Value>,
    pub unavailable_count: usize,
    pub servers: RuntimeMcpServerCounts,
    pub builtin_mcp_prompt_debug: Value,
    pub owner: &'static str,
    pub service: &'static str,
    pub runtime_scoped: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeMcpStatusPanel {
    pub servers: RuntimeMcpServerCounts,
    pub builtin_mcp_prompt_debug: Value,
    pub owner: &'static str,
    pub service: &'static str,
    pub runtime_scoped: bool,
}

/// Tool catalogs are bound to an immutable MCP Management Runtime Session.
/// A user-global preview must not initialize the retired direct MCP loaders.
pub async fn build_agent_tools_panel(_user_id: &str) -> Result<RuntimeMcpToolsPanel, String> {
    Ok(RuntimeMcpToolsPanel {
        tools: Vec::new(),
        count: 0,
        unavailable_tools: Vec::new(),
        unavailable_count: 0,
        servers: empty_server_counts(),
        builtin_mcp_prompt_debug: runtime_scoped_debug_payload(),
        owner: "mcp_management_service",
        service: "runtime_session",
        runtime_scoped: true,
    })
}

pub async fn load_agent_status_runtime_panel(_user_id: Option<String>) -> RuntimeMcpStatusPanel {
    RuntimeMcpStatusPanel {
        servers: empty_server_counts(),
        builtin_mcp_prompt_debug: runtime_scoped_debug_payload(),
        owner: "mcp_management_service",
        service: "runtime_session",
        runtime_scoped: true,
    }
}

fn empty_server_counts() -> RuntimeMcpServerCounts {
    RuntimeMcpServerCounts {
        http: 0,
        stdio: 0,
        builtin: 0,
    }
}

fn runtime_scoped_debug_payload() -> Value {
    serde_json::json!({
        "source": "mcp_management_runtime_session",
        "runtime_scoped": true,
        "message": "The effective tool catalog is available only after resolving an immutable MCP Management Runtime Session."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn global_tools_panel_does_not_initialize_direct_mcp_servers() {
        let panel = build_agent_tools_panel("user-1")
            .await
            .expect("runtime-scoped panel");

        assert!(panel.tools.is_empty());
        assert_eq!(panel.service, "runtime_session");
        assert!(panel.runtime_scoped);
        assert_eq!(panel.servers.http, 0);
        assert_eq!(panel.servers.stdio, 0);
        assert_eq!(panel.servers.builtin, 0);
    }
}
