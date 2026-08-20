// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::naming::{canonical_prefixed_tool_name, legacy_prefixed_tool_name};
use crate::rpc::{jsonrpc_http_tool_call_cancellable_with_client, jsonrpc_stdio_call};
use crate::text::{
    inject_agent_builder_args, to_text_and_structured_result_with_transient,
    to_text_and_structured_result_with_transient_limit,
};
use crate::types::{
    ToolCallContext, ToolCallError, ToolInfo, ToolLifecycleEvent, ToolLifecycleOutcome, ToolResult,
    ToolResultCallback, ToolStreamChunkCallback,
};

use super::McpExecutor;

const TASK_RUNNER_MCP_SERVER_NAME: &str = "task_runner_service";
const MCP_MANAGEMENT_SERVER_NAME: &str = "mcp_management";

impl McpExecutor {
    pub async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        if self.is_mcp_management_command(tool_calls) {
            return self
                .execute_mcp_management_batch(tool_calls, context, on_tool_result)
                .await;
        }
        if self.tool_lifecycle_hook.is_none() && self.should_parallelize_tool_batch(tool_calls) {
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
                            return Err(ToolCallError::non_fatal(reason));
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

    fn is_mcp_management_command(&self, tool_calls: &[Value]) -> bool {
        if tool_calls.is_empty() {
            return false;
        }
        let mut has_mcp_management_tool = false;
        for tool_call in tool_calls {
            let Some(name) = crate::tool_call::extract_tool_call_name(tool_call) else {
                continue;
            };
            let Some(name) = self.resolve_tool_name(name) else {
                continue;
            };
            let Some(info) = self.tool_metadata.get(name) else {
                continue;
            };
            if info.server_name != MCP_MANAGEMENT_SERVER_NAME
                || info.server_type != "http"
                || info.server_async_result_transport
                    != crate::types::McpAsyncResultTransport::RabbitMq
            {
                return false;
            }
            has_mcp_management_tool = true;
        }
        has_mcp_management_tool
    }

    async fn execute_mcp_management_batch(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        batch_error_results(
            tool_calls,
            &context,
            on_tool_result.as_ref(),
            "Managed MCP tool calls must be produced by the Cloud Agent event consumer",
            true,
        )
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
                            return Err(ToolCallError::non_fatal(reason));
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
    ) -> Result<(String, Option<Value>), ToolCallError> {
        let tool_result_max_chars = context.tool_result_max_chars.or(self.tool_result_max_chars);
        let info = self
            .tool_metadata
            .get(tool_name)
            .ok_or_else(|| format!("工具未找到: {tool_name}"))?;
        let mut lifecycle_event = ToolLifecycleEvent {
            tool_name: tool_name.to_string(),
            original_name: info.original_name.clone(),
            server_name: info.server_name.clone(),
            server_type: info.server_type.clone(),
            arguments_sha256: sha256_json(&args)?,
            outcome: None,
            result_sha256: None,
        };
        if let Some(hook) = &self.tool_lifecycle_hook {
            hook.before_tool_use(&lifecycle_event)
                .await
                .map_err(|error| {
                    ToolCallError::fatal(format!(
                        "PreToolUse Hook blocked tool {tool_name}: {error}"
                    ))
                })?;
        }
        let result = async {
            match info.server_type.as_str() {
                "http" => {
                    let url = info.server_url.clone().ok_or("missing server url")?;
                    let headers = http_tool_call_headers(info, &context).await?;
                    let result = jsonrpc_http_tool_call_cancellable_with_client(
                        url.as_str(),
                        headers.as_ref(),
                        json!({"name": info.original_name, "arguments": args}),
                        info.server_timeout,
                        info.server_async_result_transport,
                        info.server_http_client.as_ref(),
                    )
                    .await
                    .map_err(classify_remote_tool_call_error)?;
                    Ok(self.normalize_tool_result(&result, tool_result_max_chars))
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
                    Ok(self.normalize_tool_result(&result, tool_result_max_chars))
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
                    Ok(self.normalize_tool_result(&result, tool_result_max_chars))
                }
                other => Err(ToolCallError::non_fatal(format!(
                    "unsupported server type: {other}"
                ))),
            }
        }
        .await;
        lifecycle_event.outcome = Some(if result.is_ok() {
            ToolLifecycleOutcome::Succeeded
        } else {
            ToolLifecycleOutcome::Failed
        });
        lifecycle_event.result_sha256 = Some(match &result {
            Ok(value) => sha256_json(value)?,
            Err(error) => hex::encode(Sha256::digest(error.to_string().as_bytes())),
        });
        if let Some(hook) = &self.tool_lifecycle_hook {
            hook.after_tool_use(&lifecycle_event)
                .await
                .map_err(|error| {
                    ToolCallError::fatal(format!(
                        "PostToolUse Hook failed after tool {tool_name} (underlying_tool_succeeded={}): {error}",
                        result.is_ok()
                    ))
                })?;
        }
        result
    }

    fn normalize_tool_result(
        &self,
        result: &Value,
        max_chars: Option<usize>,
    ) -> (String, Option<Value>) {
        max_chars
            .map(|max_chars| to_text_and_structured_result_with_transient_limit(result, max_chars))
            .unwrap_or_else(|| to_text_and_structured_result_with_transient(result))
    }
}

fn batch_error_results(
    tool_calls: &[Value],
    context: &ToolCallContext,
    on_tool_result: Option<&ToolResultCallback>,
    message: &str,
    fatal_error: bool,
) -> Vec<ToolResult> {
    tool_calls
        .iter()
        .map(|tool_call| {
            let result = crate::execution::tool_result_error(
                crate::tool_call::extract_tool_call_id(tool_call)
                    .unwrap_or("")
                    .to_string(),
                crate::tool_call::extract_tool_call_name(tool_call)
                    .unwrap_or("unknown")
                    .to_string(),
                context.conversation_turn_id.clone(),
                format!("工具执行失败: {message}"),
                fatal_error,
            );
            if let Some(callback) = on_tool_result {
                if context.is_active() {
                    callback(&result);
                }
            }
            result
        })
        .collect()
}

fn sha256_json(value: &impl serde::Serialize) -> Result<String, ToolCallError> {
    serde_json::to_vec(value)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|error| {
            ToolCallError::non_fatal(format!("hash tool lifecycle payload failed: {error}"))
        })
}

fn classify_remote_tool_call_error(message: String) -> ToolCallError {
    ToolCallError::non_fatal(message)
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
async fn http_tool_call_headers(
    info: &ToolInfo,
    context: &ToolCallContext,
) -> Result<Option<HashMap<String, String>>, String> {
    let mut headers = info.server_headers.clone().unwrap_or_default();
    if let Some(provider) = info.server_header_provider.as_ref() {
        crate::types::extend_headers_case_insensitive(&mut headers, provider.headers().await?);
    }
    if info.server_name == TASK_RUNNER_MCP_SERVER_NAME {
        if let Some(session_id) = normalized_context_value(context.conversation_id.as_deref()) {
            headers.insert("X-Chatos-Session-Id".to_string(), session_id.clone());
            headers.insert("X-Chatos-Conversation-Id".to_string(), session_id);
        }
        if let Some(turn_id) = normalized_context_value(context.conversation_turn_id.as_deref()) {
            headers.insert("X-Chatos-Turn-Id".to_string(), turn_id);
        }
    }
    Ok((!headers.is_empty()).then_some(headers))
}
fn normalized_context_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{classify_remote_tool_call_error, McpExecutor};
    use crate::registry::BuiltinToolRegistry;
    use crate::types::{McpAsyncResultTransport, ParsedToolDefinition};
    use serde_json::json;

    #[test]
    fn ordinary_remote_tool_failure_remains_non_fatal() {
        let error = classify_remote_tool_call_error("No such file or directory".to_string());
        assert!(!error.is_fatal());
    }

    #[test]
    fn single_and_multiple_mcp_management_calls_use_the_same_command_path() {
        let mut executor = McpExecutor::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BuiltinToolRegistry::new(),
        );
        executor.register_available_tool(
            "mcp_management",
            "mcp_management",
            "http",
            Some("http://127.0.0.1/mcp".to_string()),
            None,
            None,
            None,
            McpAsyncResultTransport::RabbitMq,
            None,
            None,
            true,
            ParsedToolDefinition {
                name: "read_file".to_string(),
                description: "read".to_string(),
                parameters: json!({"type": "object"}),
            },
            json!({"name": "read_file"}),
        );
        assert!(executor.is_mcp_management_command(&[json!({
            "id": "call-1",
            "function": {"name": "read_file", "arguments": {}}
        })]));
        assert!(executor.is_mcp_management_command(&[
            json!({"id": "call-1", "function": {"name": "read_file", "arguments": {}}}),
            json!({"id": "call-2", "function": {"name": "missing_tool", "arguments": {}}}),
        ]));
    }
}
