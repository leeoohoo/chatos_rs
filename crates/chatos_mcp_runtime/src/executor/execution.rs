// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::{json, Value};
use tokio::task::JoinSet;
use tracing::warn;

use crate::naming::{canonical_prefixed_tool_name, legacy_prefixed_tool_name};
use crate::rpc::{jsonrpc_http_call, jsonrpc_stdio_call};
use crate::text::{inject_agent_builder_args, to_text_and_structured_result};
use crate::tool_call::{clone_tool_call_arguments, extract_tool_call_id, extract_tool_call_name};
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

        let mut results = Vec::new();
        for tool_call in tool_calls {
            if context.is_aborted() {
                break;
            }

            let name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
            let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
            if name.is_empty() {
                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name: "unknown".to_string(),
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content: "工具名称不能为空".to_string(),
                        result: None,
                    },
                    &context,
                    on_tool_result.as_ref(),
                );
                continue;
            }

            let args = match parse_tool_args(clone_tool_call_arguments(tool_call)) {
                Ok(value) => value,
                Err(err) => {
                    push_tool_result(
                        &mut results,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id.clone(),
                            content: format!("参数解析失败: {err}"),
                            result: None,
                        },
                        &context,
                        on_tool_result.as_ref(),
                    );
                    continue;
                }
            };
            let resolved_name = self.resolve_tool_name(name.as_str()).map(ToOwned::to_owned);
            if resolved_name.is_none() {
                if let Some(reason) =
                    unavailable_tool_reason(self.unavailable_tools.as_slice(), name.as_str())
                {
                    push_tool_result(
                        &mut results,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id.clone(),
                            content: format!("宸ュ叿鎵ц澶辫触: {reason}"),
                            result: None,
                        },
                        &context,
                        on_tool_result.as_ref(),
                    );
                    continue;
                }
            }
            let execution_name = resolved_name.unwrap_or_else(|| name.clone());

            let stream_callback = build_stream_callback(
                on_tool_result.as_ref(),
                call_id.as_str(),
                name.as_str(),
                context.conversation_turn_id.clone(),
                context.clone(),
            );
            let outcome = self
                .call_tool_once(
                    execution_name.as_str(),
                    args,
                    context.clone(),
                    stream_callback,
                )
                .await;
            if context.is_aborted() {
                break;
            }
            match outcome {
                Ok((content, structured)) => push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: true,
                        is_error: false,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content,
                        result: structured,
                    },
                    &context,
                    on_tool_result.as_ref(),
                ),
                Err(err) => {
                    warn!("[MCP] tool execution failed: tool={name}, err={err}");
                    push_tool_result(
                        &mut results,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id.clone(),
                            content: format!("工具执行失败: {err}"),
                            result: None,
                        },
                        &context,
                        on_tool_result.as_ref(),
                    );
                }
            }
        }
        results
    }
    async fn execute_tools_parallel(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        let mut ordered: Vec<Option<ToolResult>> = vec![None; tool_calls.len()];
        let mut fallbacks: Vec<Option<ToolResult>> = vec![None; tool_calls.len()];
        let mut joins: JoinSet<(usize, ToolResult)> = JoinSet::new();

        for (index, tool_call) in tool_calls.iter().enumerate() {
            if context.is_aborted() {
                break;
            }

            let name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
            let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
            fallbacks[index] = Some(parallel_tool_missing_result(
                call_id.as_str(),
                name.as_str(),
                &context,
            ));
            let args = match parse_tool_args(clone_tool_call_arguments(tool_call)) {
                Ok(value) => value,
                Err(err) => {
                    ordered[index] = Some(ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content: format!("参数解析失败: {err}"),
                        result: None,
                    });
                    continue;
                }
            };
            let resolved_name = self.resolve_tool_name(name.as_str()).map(ToOwned::to_owned);
            if resolved_name.is_none() {
                if let Some(reason) =
                    unavailable_tool_reason(self.unavailable_tools.as_slice(), name.as_str())
                {
                    ordered[index] = Some(ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id.clone(),
                        content: format!("宸ュ叿鎵ц澶辫触: {reason}"),
                        result: None,
                    });
                    continue;
                }
            }
            let execution_name = resolved_name.unwrap_or_else(|| name.clone());
            let executor = self.clone();
            let context = context.clone();
            joins.spawn(async move {
                if context.is_aborted() {
                    return (
                        index,
                        ToolResult {
                            tool_call_id: call_id,
                            name,
                            success: false,
                            is_error: true,
                            is_stream: false,
                            conversation_turn_id: context.conversation_turn_id,
                            content: "工具执行已中止".to_string(),
                            result: None,
                        },
                    );
                }

                let result = match executor
                    .call_tool_once(execution_name.as_str(), args, context.clone(), None)
                    .await
                {
                    Ok((content, structured)) => ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: true,
                        is_error: false,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id,
                        content,
                        result: structured,
                    },
                    Err(err) => ToolResult {
                        tool_call_id: call_id,
                        name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: context.conversation_turn_id,
                        content: format!("工具执行失败: {err}"),
                        result: None,
                    },
                };
                (index, result)
            });
        }

        while let Some(joined) = joins.join_next().await {
            match joined {
                Ok((index, result)) => ordered[index] = Some(result),
                Err(err) => {
                    warn!("[MCP] parallel tool task failed: {err}");
                }
            }
        }

        collect_parallel_tool_results(ordered, fallbacks, &context, on_tool_result.as_ref())
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

fn build_stream_callback(
    on_tool_result: Option<&ToolResultCallback>,
    call_id: &str,
    tool_name: &str,
    conversation_turn_id: Option<String>,
    context: ToolCallContext,
) -> Option<ToolStreamChunkCallback> {
    on_tool_result.map(|callback| {
        let callback = std::sync::Arc::clone(callback);
        let call_id = call_id.to_string();
        let tool_name = tool_name.to_string();
        std::sync::Arc::new(move |chunk: String| {
            if chunk.is_empty() {
                return;
            }
            if !context.is_active() {
                return;
            }
            callback(&ToolResult {
                tool_call_id: call_id.clone(),
                name: tool_name.clone(),
                success: true,
                is_error: false,
                is_stream: true,
                conversation_turn_id: conversation_turn_id.clone(),
                content: chunk,
                result: None,
            });
        }) as ToolStreamChunkCallback
    })
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
fn parse_tool_args(args: Value) -> Result<Value, serde_json::Error> {
    match args {
        Value::String(raw) => parse_tool_args_from_str(raw.as_str()),
        other => Ok(other),
    }
}
fn parse_tool_args_from_str(raw: &str) -> Result<Value, serde_json::Error> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return parse_nested_json_string_value(value);
    }

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::from_str::<Value>(raw);
    }

    let mut candidates = Vec::new();
    if let Some(stripped) = strip_markdown_fence(trimmed) {
        candidates.push(stripped);
    }
    if let Some(embedded) = extract_bracket_json(trimmed, '{', '}') {
        candidates.push(embedded);
    }
    if let Some(embedded) = extract_bracket_json(trimmed, '[', ']') {
        candidates.push(embedded);
    }
    candidates.push(trimmed.to_string());

    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<Value>(candidate.as_str()) {
            return parse_nested_json_string_value(value);
        }
        let repaired = remove_trailing_commas(candidate.as_str());
        if repaired != candidate {
            if let Ok(value) = serde_json::from_str::<Value>(repaired.as_str()) {
                return parse_nested_json_string_value(value);
            }
        }
    }

    serde_json::from_str::<Value>(raw)
}
fn parse_nested_json_string_value(value: Value) -> Result<Value, serde_json::Error> {
    let Some(inner) = value.as_str() else {
        return Ok(value);
    };
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    parse_tool_args_from_str(trimmed).or(Ok(value))
}
fn strip_markdown_fence(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return None;
    }
    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or_default();
    if !first_line.trim_start().starts_with("```") {
        return None;
    }

    let mut payload_lines = Vec::new();
    for line in lines {
        if line.trim_start().starts_with("```") {
            break;
        }
        payload_lines.push(line);
    }

    let joined = payload_lines.join("\n");
    let candidate = joined.trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}
fn extract_bracket_json(raw: &str, open: char, close: char) -> Option<String> {
    let start = raw.find(open)?;
    let end = raw.rfind(close)?;
    if end <= start {
        return None;
    }
    Some(raw[start..=end].trim().to_string())
}
fn remove_trailing_commas(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            out.push(ch);
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            let mut drop_comma = false;
            while let Some(next) = lookahead.peek() {
                if next.is_whitespace() {
                    lookahead.next();
                    continue;
                }
                if *next == '}' || *next == ']' {
                    drop_comma = true;
                }
                break;
            }
            if drop_comma {
                continue;
            }
        }

        out.push(ch);
    }

    out
}
fn push_tool_result(
    results: &mut Vec<ToolResult>,
    result: ToolResult,
    context: &ToolCallContext,
    on_tool_result: Option<&ToolResultCallback>,
) {
    if let Some(callback) = on_tool_result {
        if context.is_active() {
            callback(&result);
        }
    }
    results.push(result);
}
pub(in crate::executor) fn collect_parallel_tool_results(
    ordered: Vec<Option<ToolResult>>,
    fallbacks: Vec<Option<ToolResult>>,
    context: &ToolCallContext,
    on_tool_result: Option<&ToolResultCallback>,
) -> Vec<ToolResult> {
    let mut results = Vec::new();
    for (index, item) in ordered.into_iter().enumerate() {
        if let Some(item) = item.or_else(|| fallbacks.get(index).cloned().flatten()) {
            push_tool_result(&mut results, item, context, on_tool_result);
        }
    }
    results
}
pub(in crate::executor) fn parallel_tool_missing_result(
    call_id: &str,
    name: &str,
    context: &ToolCallContext,
) -> ToolResult {
    ToolResult {
        tool_call_id: call_id.to_string(),
        name: if name.is_empty() {
            "unknown".to_string()
        } else {
            name.to_string()
        },
        success: false,
        is_error: true,
        is_stream: false,
        conversation_turn_id: context.conversation_turn_id.clone(),
        content: "工具执行失败: 工具任务没有返回结果".to_string(),
        result: None,
    }
}
