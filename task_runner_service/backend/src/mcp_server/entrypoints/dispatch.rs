use super::*;

impl TaskRunnerMcpService {
    pub async fn handle_jsonrpc(
        &self,
        request: JsonRpcRequest,
        current_user: CurrentUser,
        request_context: McpRequestContext,
    ) -> JsonRpcResponse {
        let id = request.id.unwrap_or(Value::Null);
        let method = request.method.clone();
        tracing::info!(
            method = %method,
            "task runner mcp jsonrpc dispatch entered"
        );
        if request_context.is_chatos_plan_task_profile()
            && !request_context.has_concrete_project_scope()
        {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: "Chatos Plan mode requires concrete project_id".to_string(),
                }),
            };
        }
        match request.method.as_str() {
            "tools/list" => {
                tracing::info!("task runner mcp tools/list started");
                let tools = self
                    .list_tools_for_user(&current_user, request_context.tool_profile())
                    .await;
                tracing::info!(
                    tool_count = tools.len(),
                    "task runner mcp tools/list finished"
                );
                JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({ "tools": tools })),
                    error: None,
                }
            }
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
        tracing::info!(tool_name = %name, "task runner mcp tools/call started");
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.call_tool(name, args, current_user, request_context)
            .await
    }
}
