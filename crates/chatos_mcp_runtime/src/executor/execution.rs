// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::naming::{canonical_prefixed_tool_name, legacy_prefixed_tool_name};
use crate::rpc::{jsonrpc_http_tool_call_cancellable_with_client, jsonrpc_stdio_call};
use crate::text::{inject_agent_builder_args, to_text_and_structured_result_with_transient};
use crate::types::{
    ToolCallContext, ToolCallError, ToolInfo, ToolLifecycleEvent, ToolLifecycleOutcome, ToolResult,
    ToolResultCallback, ToolStreamChunkCallback,
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
                    Ok(to_text_and_structured_result_with_transient(&result))
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
                    Ok(to_text_and_structured_result_with_transient(&result))
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
                    Ok(to_text_and_structured_result_with_transient(&result))
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
}

fn sha256_json(value: &impl serde::Serialize) -> Result<String, ToolCallError> {
    serde_json::to_vec(value)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|error| {
            ToolCallError::non_fatal(format!("hash tool lifecycle payload failed: {error}"))
        })
}

fn classify_remote_tool_call_error(message: String) -> ToolCallError {
    let normalized = message.to_ascii_lowercase();
    let sandbox_lease_unavailable = normalized.contains("sandbox manager lease is not runnable")
        && (normalized.contains("destroyed") || normalized.contains("expired"));
    if sandbox_lease_unavailable {
        ToolCallError::fatal(format!(
            "sandbox infrastructure unavailable; the run must reacquire its sandbox: {message}"
        ))
    } else {
        ToolCallError::non_fatal(message)
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
    use super::classify_remote_tool_call_error;

    #[test]
    fn destroyed_or_expired_sandbox_lease_is_fatal() {
        for status in ["destroyed", "expired"] {
            let error = classify_remote_tool_call_error(format!(
                "Sandbox Manager lease is not runnable: {status}"
            ));
            assert!(error.is_fatal());
        }
    }

    #[test]
    fn ordinary_remote_tool_failure_remains_non_fatal() {
        let error = classify_remote_tool_call_error("No such file or directory".to_string());
        assert!(!error.is_fatal());
    }
}
