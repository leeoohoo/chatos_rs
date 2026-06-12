use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::core::builtin_mcp_prompt::compose_effective_builtin_mcp_system_prompt;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_runtime::{load_mcp_servers_by_selection, McpServerBundle};
use crate::core::mcp_tools::ToolInfo;
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute as AgentMcpToolExecute;

use super::snapshot::build_builtin_mcp_debug_payload;
use super::user_context::load_runtime_user_context;

#[derive(Debug)]
struct RuntimeMcpDebugContext {
    locale: InternalContextLocale,
    mcp_server_bundle: McpServerBundle,
}

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
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeMcpStatusPanel {
    pub servers: RuntimeMcpServerCounts,
    pub builtin_mcp_prompt_debug: Value,
}

pub async fn build_agent_tools_panel(user_id: &str) -> Result<RuntimeMcpToolsPanel, String> {
    let runtime_context = load_runtime_mcp_debug_context(Some(user_id.to_string())).await;
    let (http_servers, stdio_servers, builtin_servers) = runtime_context.mcp_server_bundle;
    let server_counts = RuntimeMcpServerCounts {
        http: http_servers.len(),
        stdio: stdio_servers.len(),
        builtin: builtin_servers.len(),
    };
    let mut exec = AgentMcpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    exec.init().await?;

    let tools = exec.get_tools();
    let unavailable_tools = exec.get_unavailable_tools();
    Ok(build_runtime_mcp_tools_panel(
        builtin_servers.as_slice(),
        exec.tool_metadata(),
        tools,
        unavailable_tools,
        server_counts,
        runtime_context.locale,
    ))
}

pub async fn load_agent_status_runtime_panel(user_id: Option<String>) -> RuntimeMcpStatusPanel {
    let runtime_context = load_runtime_mcp_debug_context(user_id).await;
    let (http_servers, stdio_servers, builtin_servers) = runtime_context.mcp_server_bundle;
    RuntimeMcpStatusPanel {
        servers: RuntimeMcpServerCounts {
            http: http_servers.len(),
            stdio: stdio_servers.len(),
            builtin: builtin_servers.len(),
        },
        builtin_mcp_prompt_debug: build_builtin_mcp_debug_payload(
            builtin_servers.as_slice(),
            &HashMap::<String, ToolInfo>::new(),
            &[],
            None,
            runtime_context.locale,
        ),
    }
}

async fn load_runtime_mcp_debug_context(user_id: Option<String>) -> RuntimeMcpDebugContext {
    let user_context = load_runtime_user_context(user_id, "").await;
    let mcp_server_bundle = load_mcp_servers_by_selection(
        user_context.effective_user_id.clone(),
        false,
        Vec::new(),
        None,
        None,
    )
    .await;

    RuntimeMcpDebugContext {
        locale: user_context.locale,
        mcp_server_bundle,
    }
}

fn build_runtime_mcp_tools_panel(
    builtin_servers: &[crate::services::mcp_loader::McpBuiltinServer],
    tool_metadata: &HashMap<String, ToolInfo>,
    tools: Vec<Value>,
    unavailable_tools: Vec<Value>,
    servers: RuntimeMcpServerCounts,
    locale: InternalContextLocale,
) -> RuntimeMcpToolsPanel {
    let composed_prompt = compose_effective_builtin_mcp_system_prompt(
        builtin_servers,
        tool_metadata,
        unavailable_tools.as_slice(),
        locale,
    )
    .unwrap_or_default();
    let builtin_mcp_prompt_debug = build_builtin_mcp_debug_payload(
        builtin_servers,
        tool_metadata,
        unavailable_tools.as_slice(),
        Some(composed_prompt.as_str()),
        locale,
    );

    RuntimeMcpToolsPanel {
        count: tools.len(),
        unavailable_count: unavailable_tools.len(),
        tools,
        unavailable_tools,
        servers,
        builtin_mcp_prompt_debug,
    }
}
