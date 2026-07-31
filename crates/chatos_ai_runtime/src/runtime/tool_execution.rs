// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};
use tracing::info;

use chatos_mcp_runtime::{ToolResult, ToolResultCallback};

use crate::tool_runtime::{build_tool_call_items, build_tool_output_items_for_calls_with_budget};
use crate::traits::ToolExecutor;

use super::options::AiRuntimeOptions;
use super::summaries::summarize_tool_result_names;

pub(super) struct RuntimeToolExecution {
    pub(super) tool_results: Vec<ToolResult>,
    pub(super) tool_call_items: Vec<Value>,
    pub(super) tool_output_items: Vec<Value>,
}

pub(super) fn next_consecutive_failed_tool_batch_count(
    current: usize,
    tool_results: &[ToolResult],
) -> usize {
    if !tool_results.is_empty()
        && tool_results
            .iter()
            .all(|result| result.is_error || !result.success)
    {
        current.saturating_add(1)
    } else {
        0
    }
}

pub(super) fn repeated_tool_failure_error(
    tool_results: &[ToolResult],
    failed_batch_count: usize,
) -> String {
    let last_error = tool_results
        .iter()
        .rev()
        .find(|result| result.is_error || !result.success)
        .map(|result| truncate_chars(result.content.trim(), 1_000))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "未知工具错误".to_string());
    let hint = if looks_like_missing_file_error(&last_error) {
        "最后一次失败是在读取不存在的文件；这通常不是权限问题，而是模型重复使用了无效路径。"
    } else {
        "请优先根据最后错误调整下一步，避免继续重复同一类失败工具调用。"
    };
    format!(
        "模型连续 {failed_batch_count} 轮调用工具都失败，系统已停止自动重试以避免继续循环。{hint}最后错误：{last_error}"
    )
}

pub(super) fn fatal_tool_execution_error(tool_results: &[ToolResult]) -> Option<String> {
    tool_results
        .iter()
        .find(|result| result.fatal_error)
        .map(|result| result.content.clone())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut output = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        output.push_str("...<truncated>");
    }
    output
}

fn looks_like_missing_file_error(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    [
        "no such file",
        "not found",
        "cannot find",
        "can't find",
        "could not find",
        "does not exist",
        "enoent",
        "os error 2",
        "不存在",
        "找不到",
        "未找到",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub(super) async fn execute_runtime_tools(
    executor: &dyn ToolExecutor,
    tool_calls: &Value,
    options: &AiRuntimeOptions,
    iteration: usize,
) -> Result<RuntimeToolExecution, String> {
    if let Some(cb) = &options.callbacks.on_tools_start {
        cb(tool_calls.clone());
    }
    let tool_result_callback: Option<ToolResultCallback> =
        options.callbacks.on_tools_stream.as_ref().map(|cb| {
            let cb = Arc::clone(cb);
            Arc::new(move |result: &chatos_mcp_runtime::ToolResult| {
                cb(serde_json::to_value(result).unwrap_or_else(|_| json!({})));
            }) as ToolResultCallback
        });
    let tool_call_values = tool_calls.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let started_at = Instant::now();
    let tool_results = executor
        .execute_tools_stream(
            tool_call_values,
            options.tool_call_context(),
            tool_result_callback,
        )
        .await;
    if options.is_aborted() {
        return Err("aborted".to_string());
    }
    if let Some(cb) = &options.callbacks.on_tools_end {
        cb(json!({ "tool_results": tool_results }));
    }
    if let Some(error) = fatal_tool_execution_error(tool_results.as_slice()) {
        return Err(error);
    }

    let tool_result_count = tool_results.len();
    let tool_call_items = build_tool_call_items(tool_call_values);
    let tool_output_items = build_tool_output_items_for_calls_with_budget(
        tool_call_values,
        tool_results.as_slice(),
        options.tool_result_model_budget_limits,
    );
    info!(
        conversation_id = options.conversation_id.as_deref().unwrap_or(""),
        conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
        iteration,
        tool_result_count,
        tool_result_names = summarize_tool_result_names(tool_results.as_slice(), 8).join(", "),
        tool_batch_ms = started_at.elapsed().as_millis(),
        "ai runtime finished tool execution"
    );

    Ok(RuntimeToolExecution {
        tool_results,
        tool_call_items,
        tool_output_items,
    })
}

#[cfg(test)]
mod tests {
    use chatos_mcp_runtime::ToolResult;

    use super::{
        fatal_tool_execution_error, next_consecutive_failed_tool_batch_count,
        repeated_tool_failure_error,
    };

    fn tool_result(success: bool, content: &str) -> ToolResult {
        ToolResult {
            tool_call_id: "call_1".to_string(),
            name: "demo".to_string(),
            success,
            is_error: !success,
            is_stream: false,
            conversation_turn_id: None,
            content: content.to_string(),
            result: None,
            fatal_error: false,
            transient_model_input: None,
        }
    }

    #[test]
    fn consecutive_failure_counter_resets_after_any_success() {
        assert_eq!(
            next_consecutive_failed_tool_batch_count(2, &[tool_result(false, "failed")]),
            3
        );
        assert_eq!(
            next_consecutive_failed_tool_batch_count(
                2,
                &[tool_result(false, "failed"), tool_result(true, "ok")],
            ),
            0
        );
        assert_eq!(next_consecutive_failed_tool_batch_count(2, &[]), 0);
    }

    #[test]
    fn repeated_failure_error_keeps_the_last_actionable_error() {
        let message = repeated_tool_failure_error(
            &[
                tool_result(false, "first error"),
                tool_result(false, "参数解析失败: expected comma"),
            ],
            8,
        );

        assert!(message.contains("连续 8 轮"));
        assert!(message.contains("参数解析失败: expected comma"));
        assert!(message.contains("系统已停止自动重试"));
    }

    #[test]
    fn repeated_failure_error_explains_missing_file_loops() {
        let message =
            repeated_tool_failure_error(&[tool_result(false, "pnpm-lock.yaml not found")], 8);

        assert!(message.contains("读取不存在的文件"));
        assert!(message.contains("不是权限问题"));
    }

    #[test]
    fn fatal_tool_errors_stop_the_runtime_immediately() {
        let mut result = tool_result(false, "Plugin Hook PreToolUse blocked the Run");
        result.fatal_error = true;

        assert_eq!(
            fatal_tool_execution_error(&[result]).as_deref(),
            Some("Plugin Hook PreToolUse blocked the Run")
        );
        assert!(fatal_tool_execution_error(&[tool_result(false, "ordinary tool error")]).is_none());
    }
}
