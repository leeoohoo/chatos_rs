use super::*;

impl TaskRunnerMcpService {
    pub async fn handle_jsonrpc(
        &self,
        request: JsonRpcRequest,
        current_user: CurrentUser,
        request_context: McpRequestContext,
    ) -> JsonRpcResponse {
        let id = request.id.unwrap_or(Value::Null);
        match request.method.as_str() {
            "tools/list" => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({
                    "tools": self
                        .list_tools_for_user(&current_user, request_context.tool_profile())
                        .await
                })),
                error: None,
            },
            "tools/call" => match self
                .handle_tool_call(request.params, &current_user, &request_context)
                .await
            {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(result),
                    error: None,
                },
                Err(message) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32000,
                        message,
                    }),
                },
            },
            other => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("method not found: {other}"),
                }),
            },
        }
    }

    async fn handle_tool_call(
        &self,
        params: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "tools/call.name is required".to_string())?;
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.call_tool(name, args, current_user, request_context)
            .await
    }
}
