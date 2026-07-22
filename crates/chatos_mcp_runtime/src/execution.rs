// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::task::{Id, JoinSet};
use tracing::{error, warn};

use crate::arguments::{parse_json_tool_args, parse_tool_args};
use crate::tool_call::{clone_tool_call_arguments, extract_tool_call_id, extract_tool_call_name};
use crate::types::{ToolCallContext, ToolResult, ToolResultCallback, ToolStreamChunkCallback};

const TOOL_ABORT_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub async fn execute_tool_calls_stream<F, Fut>(
    tool_calls: &[Value],
    context: ToolCallContext,
    on_tool_result: Option<ToolResultCallback>,
    mut call_tool_once: F,
) -> Vec<ToolResult>
where
    F: FnMut(String, Value, Option<ToolStreamChunkCallback>) -> Fut,
    Fut: Future<Output = Result<(String, Option<Value>), String>>,
{
    let mut results = Vec::new();

    for tool_call in tool_calls {
        if context.is_aborted() {
            break;
        }

        let tool_name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
        let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();

        if tool_name.is_empty() {
            push_tool_result(
                &mut results,
                tool_result_error(
                    call_id,
                    "unknown".to_string(),
                    context.conversation_turn_id.clone(),
                    "工具名称不能为空".to_string(),
                ),
                &context,
                on_tool_result.as_ref(),
            );
            continue;
        }

        let args = match parse_tool_args(clone_tool_call_arguments(tool_call)) {
            Ok(value) => value,
            Err(err) => {
                warn!(
                    "[MCP] parse tool args failed: tool={}, call_id={}, err={}",
                    tool_name, call_id, err
                );
                push_tool_result(
                    &mut results,
                    tool_result_error(
                        call_id,
                        tool_name,
                        context.conversation_turn_id.clone(),
                        format!("参数解析失败: {err}"),
                    ),
                    &context,
                    on_tool_result.as_ref(),
                );
                continue;
            }
        };

        let stream_callback = build_stream_callback(
            on_tool_result.as_ref(),
            call_id.as_str(),
            tool_name.as_str(),
            &context,
        );
        let outcome = call_tool_once(tool_name.clone(), args, stream_callback);
        tokio::pin!(outcome);
        let outcome = loop {
            tokio::select! {
                outcome = &mut outcome => break Some(outcome),
                _ = tokio::time::sleep(TOOL_ABORT_POLL_INTERVAL) => {
                    if context.is_aborted() {
                        break None;
                    }
                }
            }
        };
        let Some(outcome) = outcome else {
            break;
        };
        if context.is_aborted() {
            break;
        }

        match outcome {
            Ok((content, structured_result)) => push_tool_result(
                &mut results,
                tool_result_success(
                    call_id,
                    tool_name,
                    context.conversation_turn_id.clone(),
                    content,
                    structured_result,
                ),
                &context,
                on_tool_result.as_ref(),
            ),
            Err(err) => {
                if err == "aborted" {
                    break;
                }
                warn!(
                    "[MCP] tool execution failed: tool={}, call_id={}, err={}",
                    tool_name, call_id, err
                );
                push_tool_result(
                    &mut results,
                    tool_result_error(
                        call_id,
                        tool_name,
                        context.conversation_turn_id.clone(),
                        format!("工具执行失败: {err}"),
                    ),
                    &context,
                    on_tool_result.as_ref(),
                );
            }
        }
    }

    results
}

pub async fn execute_tool_calls_parallel<E, Fut>(
    tool_calls: &[Value],
    context: ToolCallContext,
    on_tool_result: Option<ToolResultCallback>,
    execute_one: E,
) -> Vec<ToolResult>
where
    E: Fn(String, Value, ToolCallContext, Option<ToolStreamChunkCallback>) -> Fut
        + Clone
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<(String, Option<Value>), String>> + Send + 'static,
{
    let mut results: Vec<Option<ToolResult>> = vec![None; tool_calls.len()];
    let mut join_set: JoinSet<(usize, ToolResult)> = JoinSet::new();
    let mut pending_contexts: HashMap<Id, PendingToolContext> = HashMap::new();

    for (index, tool_call) in tool_calls.iter().enumerate() {
        if context.is_aborted() {
            break;
        }

        let tool_name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
        let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
        if tool_name.is_empty() {
            results[index] = Some(tool_result_error(
                call_id,
                "unknown".to_string(),
                context.conversation_turn_id.clone(),
                "工具名称不能为空".to_string(),
            ));
            continue;
        }

        let args = match parse_json_tool_args(clone_tool_call_arguments(tool_call)) {
            Ok(value) => value,
            Err(err) => {
                results[index] = Some(tool_result_error(
                    call_id,
                    tool_name,
                    context.conversation_turn_id.clone(),
                    format!("参数解析失败: {err}"),
                ));
                continue;
            }
        };

        let stream_callback = build_stream_callback(
            on_tool_result.as_ref(),
            call_id.as_str(),
            tool_name.as_str(),
            &context,
        );
        let execute_one = execute_one.clone();
        let task_context = context.clone();
        let pending_call_id = call_id.clone();
        let pending_tool_name = tool_name.clone();
        let pending_turn_id = context.conversation_turn_id.clone();
        let task = join_set.spawn(async move {
            let result = if task_context.is_aborted() {
                tool_result_error(
                    call_id,
                    tool_name,
                    task_context.conversation_turn_id.clone(),
                    "工具执行已中止".to_string(),
                )
            } else {
                match execute_one(
                    tool_name.clone(),
                    args,
                    task_context.clone(),
                    stream_callback,
                )
                .await
                {
                    Ok((content, structured_result)) => tool_result_success(
                        call_id,
                        tool_name,
                        task_context.conversation_turn_id.clone(),
                        content,
                        structured_result,
                    ),
                    Err(err) => tool_result_error(
                        call_id,
                        tool_name,
                        task_context.conversation_turn_id.clone(),
                        format!("工具执行失败: {err}"),
                    ),
                }
            };
            (index, result)
        });
        pending_contexts.insert(
            task.id(),
            PendingToolContext {
                index,
                call_id: pending_call_id,
                tool_name: pending_tool_name,
                conversation_turn_id: pending_turn_id,
            },
        );
    }

    while !join_set.is_empty() {
        let joined = tokio::select! {
            joined = join_set.join_next_with_id() => joined,
            _ = tokio::time::sleep(TOOL_ABORT_POLL_INTERVAL) => {
                if context.is_aborted() {
                    join_set.abort_all();
                    break;
                }
                continue;
            }
        };
        let Some(joined) = joined else {
            break;
        };
        match joined {
            Ok((task_id, (index, result))) => {
                pending_contexts.remove(&task_id);
                results[index] = Some(result);
            }
            Err(err) => {
                let task_id = err.id();
                if let Some(pending) = pending_contexts.remove(&task_id) {
                    error!(
                        tool_name = %pending.tool_name,
                        tool_call_id = %pending.call_id,
                        "parallel tool task panicked or was cancelled: {}",
                        err
                    );
                    results[pending.index] = Some(tool_result_error(
                        pending.call_id,
                        pending.tool_name,
                        pending.conversation_turn_id,
                        join_error_message(&err),
                    ));
                } else {
                    warn!("parallel tool join error without context: {err}");
                }
            }
        }

        if context.is_aborted() {
            join_set.abort_all();
            break;
        }
    }

    let mut ordered_results = Vec::new();
    for result in results.into_iter().flatten() {
        if !context.is_active() {
            break;
        }
        push_tool_result(
            &mut ordered_results,
            result,
            &context,
            on_tool_result.as_ref(),
        );
    }
    ordered_results
}

fn build_stream_callback(
    on_tool_result: Option<&ToolResultCallback>,
    call_id: &str,
    tool_name: &str,
    context: &ToolCallContext,
) -> Option<ToolStreamChunkCallback> {
    on_tool_result.map(|callback| {
        let callback = Arc::clone(callback);
        let call_id = call_id.to_string();
        let tool_name = tool_name.to_string();
        let context = context.clone();
        Arc::new(move |chunk: String| {
            if chunk.is_empty() || !context.is_active() {
                return;
            }
            callback(&ToolResult {
                tool_call_id: call_id.clone(),
                name: tool_name.clone(),
                success: true,
                is_error: false,
                is_stream: true,
                conversation_turn_id: context.conversation_turn_id.clone(),
                content: chunk,
                result: None,
            });
        }) as ToolStreamChunkCallback
    })
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

fn tool_result_success(
    tool_call_id: String,
    name: String,
    conversation_turn_id: Option<String>,
    content: String,
    result: Option<Value>,
) -> ToolResult {
    ToolResult {
        tool_call_id,
        name,
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id,
        content,
        result,
    }
}

fn tool_result_error(
    tool_call_id: String,
    name: String,
    conversation_turn_id: Option<String>,
    content: String,
) -> ToolResult {
    ToolResult {
        tool_call_id,
        name,
        success: false,
        is_error: true,
        is_stream: false,
        conversation_turn_id,
        content,
        result: None,
    }
}

fn join_error_message(err: &tokio::task::JoinError) -> String {
    if err.is_cancelled() {
        "工具执行失败: parallel task cancelled before completion".to_string()
    } else if err.is_panic() {
        "工具执行失败: internal panic in parallel tool execution".to_string()
    } else {
        format!("工具执行失败: parallel tool task join error: {err}")
    }
}

struct PendingToolContext {
    index: usize,
    call_id: String,
    tool_name: String,
    conversation_turn_id: Option<String>,
}

#[cfg(test)]
mod tests;
