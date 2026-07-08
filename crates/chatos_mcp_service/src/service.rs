// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::{json, Value};

use crate::protocol::{
    jsonrpc_error, jsonrpc_ok, JsonRpcRequest, JsonRpcResponse, MCP_ERROR_INTERNAL,
    MCP_ERROR_INVALID_PARAMS, MCP_ERROR_METHOD_NOT_FOUND, METHOD_INITIALIZE,
    METHOD_NOTIFICATIONS_INITIALIZED, METHOD_PING, METHOD_TOOLS_CALL, METHOD_TOOLS_LIST,
};
use crate::provider::{McpRequestContext, McpToolProvider};

#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
}

impl McpServerInfo {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            protocol_version: "2024-11-05".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct McpJsonRpcService {
    server_info: McpServerInfo,
    provider: Arc<dyn McpToolProvider>,
}

impl McpJsonRpcService {
    pub fn new(server_info: McpServerInfo, provider: Arc<dyn McpToolProvider>) -> Self {
        Self {
            server_info,
            provider,
        }
    }

    pub async fn handle(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        self.handle_with_context(request, McpRequestContext::default())
            .await
    }

    pub async fn handle_with_context(
        &self,
        request: JsonRpcRequest,
        context: McpRequestContext,
    ) -> JsonRpcResponse {
        let id = request.id.unwrap_or(Value::Null);
        match request.method.as_str() {
            METHOD_INITIALIZE => jsonrpc_ok(id, self.initialize_result()),
            METHOD_NOTIFICATIONS_INITIALIZED | METHOD_PING => jsonrpc_ok(id, json!({})),
            METHOD_TOOLS_LIST => {
                let tools = self.provider.list_tools(&context);
                jsonrpc_ok(id, json!({ "tools": tools }))
            }
            METHOD_TOOLS_CALL => self.handle_tool_call(id, request.params, context).await,
            other => jsonrpc_error(
                id,
                MCP_ERROR_METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            ),
        }
    }

    async fn handle_tool_call(
        &self,
        id: Value,
        params: Value,
        context: McpRequestContext,
    ) -> JsonRpcResponse {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(name) = name else {
            return jsonrpc_error(id, MCP_ERROR_INVALID_PARAMS, "tools/call.name is required");
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match self.provider.call_tool(name, args, context).await {
            Ok(result) => jsonrpc_ok(id, result),
            Err(message) => jsonrpc_error(id, MCP_ERROR_INTERNAL, message),
        }
    }

    fn initialize_result(&self) -> Value {
        json!({
            "protocolVersion": self.server_info.protocol_version,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": self.server_info.name,
                "version": self.server_info.version
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::{json, Value};

    use super::*;

    struct FakeProvider;

    #[async_trait]
    impl McpToolProvider for FakeProvider {
        fn server_name(&self) -> &str {
            "fake"
        }

        fn list_tools(&self, _context: &McpRequestContext) -> Vec<Value> {
            vec![json!({
                "name": "echo",
                "description": "Echo input",
                "inputSchema": { "type": "object" }
            })]
        }

        async fn call_tool(
            &self,
            name: &str,
            args: Value,
            _context: McpRequestContext,
        ) -> Result<Value, String> {
            if name == "echo" {
                Ok(json!({ "ok": true, "args": args }))
            } else {
                Err(format!("tool not found: {name}"))
            }
        }
    }

    fn service() -> McpJsonRpcService {
        McpJsonRpcService::new(McpServerInfo::new("fake", "0.1.0"), Arc::new(FakeProvider))
    }

    #[tokio::test]
    async fn handles_initialize() {
        let response = service()
            .handle(JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("init-1")),
                method: METHOD_INITIALIZE.to_string(),
                params: json!({}),
            })
            .await;
        assert!(response.error.is_none());
        assert_eq!(
            response
                .result
                .as_ref()
                .and_then(|value| value.get("serverInfo"))
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str),
            Some("fake")
        );
    }

    #[tokio::test]
    async fn handles_ping() {
        let response = service()
            .handle(JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("ping-1")),
                method: METHOD_PING.to_string(),
                params: json!({}),
            })
            .await;
        assert_eq!(response.result, Some(json!({})));
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn handles_tools_list() {
        let response = service()
            .handle(JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!(1)),
                method: METHOD_TOOLS_LIST.to_string(),
                params: json!({}),
            })
            .await;
        assert!(response.error.is_none());
        assert_eq!(
            response
                .result
                .as_ref()
                .and_then(|value| value.get("tools"))
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }

    #[tokio::test]
    async fn rejects_tools_call_without_name() {
        let response = service()
            .handle(JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("call-1")),
                method: METHOD_TOOLS_CALL.to_string(),
                params: json!({}),
            })
            .await;
        assert_eq!(
            response.error.as_ref().map(|error| error.code),
            Some(MCP_ERROR_INVALID_PARAMS)
        );
    }

    #[tokio::test]
    async fn handles_tools_call() {
        let response = service()
            .handle(JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!("call-2")),
                method: METHOD_TOOLS_CALL.to_string(),
                params: json!({ "name": "echo", "arguments": { "value": 1 } }),
            })
            .await;
        assert_eq!(
            response.result,
            Some(json!({ "ok": true, "args": { "value": 1 } }))
        );
    }

    #[tokio::test]
    async fn rejects_unknown_method() {
        let response = service()
            .handle(JsonRpcRequest {
                jsonrpc: Some("2.0".to_string()),
                id: Some(json!(3)),
                method: "missing".to_string(),
                params: json!({}),
            })
            .await;
        assert_eq!(
            response.error.as_ref().map(|error| error.code),
            Some(MCP_ERROR_METHOD_NOT_FOUND)
        );
    }
}
