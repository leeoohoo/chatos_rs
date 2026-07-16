// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::naming::{canonical_prefixed_tool_name, legacy_prefixed_tool_name};
use crate::rpc::{jsonrpc_http_call, jsonrpc_stdio_call};
use crate::text::{inject_agent_builder_args, to_text_and_structured_result};
use crate::types::{
    ToolCallContext, ToolInfo, ToolResult, ToolResultCallback, ToolStreamChunkCallback,
};

use super::McpExecutor;

const TASK_RUNNER_MCP_SERVER_NAME: &str = "task_runner_service";

impl McpExecutor {
    pub async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        if self.should_parallelize_tool_batch(tool_calls) {
            return self
                .execute_tools_parallel(tool_calls, context, on_tool_result)
                .await;
        }

        let execution_context = context.clone();
        crate::execution::execute_tool_calls_stream(
            tool_calls,
            context,
            on_tool_result,
            |name, args, stream_callback| {
                let context = execution_context.clone();
                async move {
                    let resolved_name =
                        self.resolve_tool_name(name.as_str()).map(ToOwned::to_owned);
                    if resolved_name.is_none() {
                        if let Some(reason) = unavailable_tool_reason(
                            self.unavailable_tools.as_slice(),
                            name.as_str(),
                        ) {
                            return Err(reason);
                        }
                    }
                    let execution_name = resolved_name.unwrap_or(name);
                    self.call_tool_once(execution_name.as_str(), args, context, stream_callback)
                        .await
                }
            },
        )
        .await
    }
    async fn execute_tools_parallel(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        let executor = self.clone();
        crate::execution::execute_tool_calls_parallel(
            tool_calls,
            context,
            on_tool_result,
            move |name, args, context, _stream_callback| {
                let executor = executor.clone();
                async move {
                    let resolved_name = executor
                        .resolve_tool_name(name.as_str())
                        .map(ToOwned::to_owned);
                    if resolved_name.is_none() {
                        if let Some(reason) = unavailable_tool_reason(
                            executor.unavailable_tools.as_slice(),
                            name.as_str(),
                        ) {
                            return Err(reason);
                        }
                    }
                    let execution_name = resolved_name.unwrap_or(name);
                    executor
                        .call_tool_once(execution_name.as_str(), args, context, None)
                        .await
                }
            },
        )
        .await
    }
    async fn call_tool_once(
        &self,
        tool_name: &str,
        args: Value,
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<(String, Option<Value>), String> {
        let info = self
            .tool_metadata
            .get(tool_name)
            .ok_or_else(|| format!("工具未找到: {tool_name}"))?;
        match info.server_type.as_str() {
            "http" => {
                let url = info.server_url.clone().ok_or("missing server url")?;
                let headers = http_tool_call_headers(info, &context);
                let result = jsonrpc_http_call(
                    url.as_str(),
                    headers.as_ref(),
                    "tools/call",
                    json!({"name": info.original_name, "arguments": args}),
                    info.server_timeout,
                )
                .await?;
                Ok(to_text_and_structured_result(&result))
            }
            "stdio" => {
                let config = info.server_config.clone().ok_or("missing server config")?;
                let result = jsonrpc_stdio_call(
                    &config,
                    "tools/call",
                    json!({"name": info.original_name, "arguments": args}),
                    context.conversation_id.as_deref(),
                )
                .await?;
                Ok(to_text_and_structured_result(&result))
            }
            "builtin" => {
                let provider = self
                    .builtin_registry
                    .get(info.server_name.as_str())
                    .ok_or_else(|| "missing builtin provider".to_string())?;
                let args = if info.server_name == "agent_builder" {
                    inject_agent_builder_args(args, context.caller_model.as_deref())
                } else {
                    args
                };
                let result = provider
                    .call_tool(info.original_name.as_str(), args, context, on_stream_chunk)
                    .await?;
                Ok(to_text_and_structured_result(&result))
            }
            other => Err(format!("unsupported server type: {other}")),
        }
    }
}

fn unavailable_tool_reason(unavailable_tools: &[Value], full_tool_name: &str) -> Option<String> {
    unavailable_tools.iter().find_map(|item| {
        let server_name = item
            .get("server_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let tool_name = item
            .get("tool_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let canonical = canonical_prefixed_tool_name(server_name, tool_name);
        let legacy = legacy_prefixed_tool_name(server_name, tool_name);
        ([canonical.as_str(), legacy.as_str()].contains(&full_tool_name)).then(|| {
            item.get("reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("tool is unavailable")
                .to_string()
        })
    })
}
fn http_tool_call_headers(
    info: &ToolInfo,
    context: &ToolCallContext,
) -> Option<HashMap<String, String>> {
    let mut headers = info.server_headers.clone().unwrap_or_default();
    if info.server_name == TASK_RUNNER_MCP_SERVER_NAME {
        if let Some(session_id) = normalized_context_value(context.conversation_id.as_deref()) {
            headers.insert("X-Chatos-Session-Id".to_string(), session_id.clone());
            headers.insert("X-Chatos-Conversation-Id".to_string(), session_id);
        }
        if let Some(turn_id) = normalized_context_value(context.conversation_turn_id.as_deref()) {
            headers.insert("X-Chatos-Turn-Id".to_string(), turn_id);
        }
    }
    (!headers.is_empty()).then_some(headers)
}
fn normalized_context_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
