use std::collections::HashMap;

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::core::mcp_tools::{
    build_builtin_tool_service, build_function_tool_schema,
    execute_tools_stream as execute_tools_stream_common, inject_sub_agent_router_args,
    jsonrpc_http_call, jsonrpc_stdio_call, list_tools_http, list_tools_stdio,
    parse_tool_definition, to_text, BuiltinToolService, ToolResultCallback, ToolSchemaFormat,
    ToolStreamChunkCallback,
};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

pub use crate::core::mcp_tools::{ToolInfo, ToolResult};

#[derive(Clone)]
pub struct McpToolExecute {
    pub mcp_servers: Vec<McpHttpServer>,
    pub stdio_mcp_servers: Vec<McpStdioServer>,
    pub builtin_mcp_servers: Vec<McpBuiltinServer>,
    pub tools: Vec<Value>,
    pub tool_metadata: HashMap<String, ToolInfo>,
    builtin_services: HashMap<String, BuiltinToolService>,
}

impl McpToolExecute {
    pub fn new(
        mcp_servers: Vec<McpHttpServer>,
        stdio_mcp_servers: Vec<McpStdioServer>,
        builtin_mcp_servers: Vec<McpBuiltinServer>,
    ) -> Self {
        Self {
            mcp_servers,
            stdio_mcp_servers,
            builtin_mcp_servers,
            tools: Vec::new(),
            tool_metadata: HashMap::new(),
            builtin_services: HashMap::new(),
        }
    }

    pub async fn init(&mut self) -> Result<(), String> {
        self.build_tools().await
    }

    pub async fn build_tools(&mut self) -> Result<(), String> {
        self.tools.clear();
        self.tool_metadata.clear();
        self.builtin_services.clear();

        let http_servers = self.mcp_servers.clone();
        for server in &http_servers {
            if let Err(err) = self.build_tools_from_http(server).await {
                warn!("failed to build tools from http {}: {}", server.name, err);
            }
        }

        let stdio_servers = self.stdio_mcp_servers.clone();
        for server in &stdio_servers {
            if let Err(err) = self.build_tools_from_stdio(server).await {
                warn!("failed to build tools from stdio {}: {}", server.name, err);
            }
        }

        let builtin_servers = self.builtin_mcp_servers.clone();
        for server in &builtin_servers {
            if let Err(err) = self.build_tools_from_builtin(server) {
                warn!(
                    "failed to build tools from builtin {}: {}",
                    server.name, err
                );
            }
        }

        info!("MCP tools built: {}", self.tools.len());
        Ok(())
    }

    async fn build_tools_from_http(&mut self, server: &McpHttpServer) -> Result<(), String> {
        let tools = list_tools_http(&server.url).await?;
        for tool in tools {
            self.register_tool(
                &server.name,
                "http",
                Some(server.url.clone()),
                None,
                tool,
                ToolSchemaFormat::ResponsesStrict,
            );
        }

        Ok(())
    }

    async fn build_tools_from_stdio(&mut self, server: &McpStdioServer) -> Result<(), String> {
        let tools = list_tools_stdio(server).await?;
        for tool in tools {
            self.register_tool(
                &server.name,
                "stdio",
                None,
                Some(server.clone()),
                tool,
                ToolSchemaFormat::ResponsesStrict,
            );
        }

        Ok(())
    }

    fn build_tools_from_builtin(&mut self, server: &McpBuiltinServer) -> Result<(), String> {
        let service = build_builtin_tool_service(server)?;
        let tools = service.list_tools();

        self.builtin_services.insert(server.name.clone(), service);

        for tool in tools {
            self.register_tool(
                &server.name,
                "builtin",
                None,
                None,
                tool,
                ToolSchemaFormat::ResponsesStrict,
            );
        }

        Ok(())
    }

    fn register_tool(
        &mut self,
        server_name: &str,
        server_type: &str,
        server_url: Option<String>,
        server_config: Option<McpStdioServer>,
        tool: Value,
        schema_format: ToolSchemaFormat,
    ) {
        let Some(definition) = parse_tool_definition(&tool) else {
            return;
        };

        let prefixed_name = format!("{}_{}", server_name, definition.name);
        self.tools.push(build_function_tool_schema(
            &prefixed_name,
            &definition.description,
            &definition.parameters,
            schema_format,
        ));

        self.tool_metadata.insert(
            prefixed_name,
            ToolInfo {
                original_name: definition.name,
                server_name: server_name.to_string(),
                server_type: server_type.to_string(),
                server_url,
                server_config,
                tool_info: tool,
            },
        );
    }

    pub fn get_available_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    pub fn get_tools(&self) -> Vec<Value> {
        self.get_available_tools()
    }

    pub async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        execute_tools_stream_common(
            tool_calls,
            session_id,
            on_tool_result,
            |tool_name, args, on_stream_chunk| async move {
                self.call_tool_once(
                    tool_name.as_str(),
                    args,
                    session_id,
                    conversation_turn_id,
                    caller_model,
                    on_stream_chunk,
                )
                .await
            },
        )
        .await
    }

    async fn call_tool_once(
        &self,
        tool_name: &str,
        args: Value,
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<String, String> {
        let info = self
            .tool_metadata
            .get(tool_name)
            .ok_or_else(|| format!("工具未找到: {}", tool_name))?;

        if info.server_type == "http" {
            let url = info.server_url.clone().ok_or("missing server url")?;
            let result = jsonrpc_http_call(
                &url,
                "tools/call",
                json!({"name": info.original_name, "arguments": args}),
            )
            .await?;
            Ok(to_text(&result))
        } else if info.server_type == "builtin" {
            let service = self
                .builtin_services
                .get(&info.server_name)
                .ok_or_else(|| "missing builtin service".to_string())?;

            let args = if matches!(service, BuiltinToolService::SubAgentRouter(_)) {
                inject_sub_agent_router_args(args, caller_model)
            } else {
                args
            };

            let result = service.call_tool(
                &info.original_name,
                args,
                session_id,
                conversation_turn_id,
                on_stream_chunk,
            )?;
            Ok(to_text(&result))
        } else {
            let config = info.server_config.clone().ok_or("missing server config")?;
            let result = jsonrpc_stdio_call(
                &config,
                "tools/call",
                json!({"name": info.original_name, "arguments": args}),
                session_id,
            )
            .await?;
            Ok(to_text(&result))
        }
    }
}
