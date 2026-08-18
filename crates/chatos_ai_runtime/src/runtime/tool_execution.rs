// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};
use tracing::info;
use uuid::Uuid;

use chatos_mcp_runtime::{ToolResult, ToolResultCallback};

#[cfg(test)]
use crate::file_write_recovery::automatic_file_write_recovery_calls;
use crate::tool_runtime::{build_tool_call_items, build_tool_output_items_for_calls_with_budget};
use crate::traits::ToolExecutor;

use super::options::AiRuntimeOptions;
use super::summaries::summarize_tool_result_names;

pub(super) struct RuntimeToolExecution {
    pub(super) tool_results: Vec<ToolResult>,
    pub(super) tool_calls: Vec<Value>,
    pub(super) tool_call_items: Vec<Value>,
    pub(super) tool_output_items: Vec<Value>,
}

impl RuntimeToolExecution {
    pub(super) fn extend(&mut self, other: Self) {
        self.tool_results.extend(other.tool_results);
        self.tool_calls.extend(other.tool_calls);
        self.tool_call_items.extend(other.tool_call_items);
        self.tool_output_items.extend(other.tool_output_items);
    }
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
    let (instrumented_tool_calls, invocation_ids) = instrument_tool_calls(tool_calls)?;
    if let Some(cb) = &options.callbacks.on_tools_start {
        cb(instrumented_tool_calls);
    }
    let tool_result_callback: Option<ToolResultCallback> =
        options.callbacks.on_tools_stream.as_ref().map(|cb| {
            let cb = Arc::clone(cb);
            let invocation_ids = Arc::clone(&invocation_ids);
            Arc::new(move |result: &chatos_mcp_runtime::ToolResult| {
                let Some(invocation_id) = invocation_ids.get(result.tool_call_id.as_str()) else {
                    tracing::warn!(
                        tool_call_id = result.tool_call_id.as_str(),
                        tool_name = result.name.as_str(),
                        "ignored orphan tool result without a registered invocation"
                    );
                    return;
                };
                cb(instrument_tool_result(result, invocation_id));
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
    validate_terminal_tool_results(tool_results.as_slice(), invocation_ids.as_ref())?;
    if let Some(cb) = &options.callbacks.on_tools_end {
        cb(json!({
            "tool_results": tool_results
                .iter()
                .map(|result| instrument_tool_result(
                    result,
                    invocation_ids
                        .get(result.tool_call_id.as_str())
                        .expect("validated invocation id"),
                ))
                .collect::<Vec<_>>(),
        }));
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
        tool_calls: tool_call_values.to_vec(),
        tool_call_items,
        tool_output_items,
    })
}

fn instrument_tool_calls(
    tool_calls: &Value,
) -> Result<(Value, Arc<HashMap<String, String>>), String> {
    let calls = tool_calls
        .as_array()
        .ok_or_else(|| "tool call payload must be an array".to_string())?;
    let mut instrumented = Vec::with_capacity(calls.len());
    let mut invocation_ids = HashMap::with_capacity(calls.len());
    for call in calls {
        let call_id = crate::tool_call::extract_tool_call_id(call)
            .ok_or_else(|| "tool call is missing a stable call id".to_string())?;
        if invocation_ids.contains_key(call_id) {
            return Err(format!("duplicate tool call id in one batch: {call_id}"));
        }
        let invocation_id = Uuid::new_v4().to_string();
        let mut value = call.clone();
        let object = value
            .as_object_mut()
            .ok_or_else(|| "tool call must be a JSON object".to_string())?;
        object.insert(
            "invocation_id".to_string(),
            Value::String(invocation_id.clone()),
        );
        invocation_ids.insert(call_id.to_string(), invocation_id);
        instrumented.push(value);
    }
    Ok((Value::Array(instrumented), Arc::new(invocation_ids)))
}

fn instrument_tool_result(result: &ToolResult, invocation_id: &str) -> Value {
    let mut value = serde_json::to_value(result).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "invocation_id".to_string(),
            Value::String(invocation_id.to_string()),
        );
    }
    value
}

fn validate_terminal_tool_results(
    results: &[ToolResult],
    invocation_ids: &HashMap<String, String>,
) -> Result<(), String> {
    let mut completed = HashSet::with_capacity(results.len());
    for result in results {
        let invocation_id = invocation_ids
            .get(result.tool_call_id.as_str())
            .ok_or_else(|| format!("orphan tool result: {}", result.tool_call_id))?;
        if !completed.insert(invocation_id.as_str()) {
            return Err(format!(
                "duplicate terminal tool result for invocation {invocation_id}"
            ));
        }
    }
    if completed.len() != invocation_ids.len() {
        return Err(format!(
            "tool invocation/result mismatch: expected {}, received {}",
            invocation_ids.len(),
            completed.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chatos_mcp_runtime::ToolResult;

    use super::{
        automatic_file_write_recovery_calls, fatal_tool_execution_error, instrument_tool_calls,
        next_consecutive_failed_tool_batch_count, repeated_tool_failure_error,
        validate_terminal_tool_results,
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
    fn stale_code_maintainer_write_builds_a_scoped_automatic_read() {
        let mut stale = tool_result(
            false,
            r#"tool failed: {"category":"stale_context","path":"README.md","recovery":{"required_next_tool":"read_file_raw","recommended_args":{"path":"README.md"}}}"#,
        );
        stale.name = "code_maintainer_write_commit_edit_session".to_string();
        let tools = vec![
            serde_json::json!({"name": "code_maintainer_write_commit_edit_session"}),
            serde_json::json!({"name": "code_maintainer_read_read_file_raw"}),
        ];

        let calls = automatic_file_write_recovery_calls(&[stale], tools.as_slice())
            .expect("automatic recovery calls");

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0]["function"]["name"],
            "code_maintainer_read_read_file_raw"
        );
        let args: serde_json::Value = serde_json::from_str(
            calls[0]["function"]["arguments"]
                .as_str()
                .expect("arguments"),
        )
        .expect("recovery args");
        assert_eq!(args, serde_json::json!({"path": "README.md"}));
    }

    #[test]
    fn missing_read_recovery_is_an_immediate_capability_invariant_error() {
        let mut stale = tool_result(
            false,
            r#"{"category":"stale_context","path":"README.md","recovery":{"required_next_tool":"read_file_raw"}}"#,
        );
        stale.name = "code_maintainer_write_stage_edit_batch".to_string();

        let error = automatic_file_write_recovery_calls(
            &[stale],
            &[serde_json::json!({"name": "unrelated_read_file_raw"})],
        )
        .expect_err("missing required read capability must fail immediately");

        assert!(error.contains("MCP capability invariant violated"));
        assert!(error.contains("read_file_raw"));
    }

    #[test]
    fn multi_conflict_commit_builds_multiple_recovery_reads() {
        let mut stale = tool_result(
            false,
            r#"{"category":"stale_context","error":"conflict","conflicts":[{"path":"src/a.rs","recovery":{"required_next_tool":"read_file_raw","recommended_args":{"path":"src/a.rs"}}},{"path":"src/b.rs","recovery":{"required_next_tool":"read_file_raw","recommended_args":{"path":"src/b.rs"}}}]}"#,
        );
        stale.name = "code_maintainer_write_commit_edit_session".to_string();
        let tools = vec![serde_json::json!({"name": "code_maintainer_read_read_file_raw"})];

        let calls = automatic_file_write_recovery_calls(&[stale], tools.as_slice())
            .expect("automatic recovery calls");

        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn expected_match_write_failure_builds_a_scoped_automatic_read() {
        let mut expected_match = tool_result(
            false,
            r#"tool failed: {"category":"expected_match","path":"design/svg/validate.mjs","latest_sha256":"abc123","recovery":{"required_next_tool":"read_file_raw","recommended_args":{"path":"design/svg/validate.mjs"}},"candidate_summary":{"count":0,"candidates":[]}}"#,
        );
        expected_match.name = "code_maintainer_write_stage_edit_batch".to_string();
        let tools = vec![
            serde_json::json!({"name": "code_maintainer_write_stage_edit_batch"}),
            serde_json::json!({"name": "code_maintainer_read_read_file_raw"}),
        ];

        let calls = automatic_file_write_recovery_calls(&[expected_match], tools.as_slice())
            .expect("automatic recovery calls");

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0]["function"]["name"],
            "code_maintainer_read_read_file_raw"
        );
        let args: serde_json::Value = serde_json::from_str(
            calls[0]["function"]["arguments"]
                .as_str()
                .expect("arguments"),
        )
        .expect("recovery args");
        assert_eq!(args, serde_json::json!({"path": "design/svg/validate.mjs"}));
    }

    #[test]
    fn tool_invocations_are_unique_and_terminal_results_must_pair_exactly() {
        let calls = serde_json::json!([
            {"id": "call-1", "function": {"name": "read_file", "arguments": "{}"}},
            {"id": "call-2", "function": {"name": "read_file", "arguments": "{}"}}
        ]);
        let (instrumented, invocation_ids) = instrument_tool_calls(&calls).expect("instrument");
        let items = instrumented.as_array().expect("instrumented calls");
        assert_ne!(items[0]["invocation_id"], items[1]["invocation_id"]);

        let mut first = tool_result(true, "ok");
        first.tool_call_id = "call-1".to_string();
        let mut second = tool_result(true, "ok");
        second.tool_call_id = "call-2".to_string();
        validate_terminal_tool_results(&[first.clone(), second], invocation_ids.as_ref())
            .expect("paired results");

        let error = validate_terminal_tool_results(&[first], invocation_ids.as_ref())
            .expect_err("missing result must fail");
        assert!(error.contains("expected 2, received 1"));
    }

    #[test]
    fn duplicate_tool_call_ids_are_rejected_before_execution() {
        let calls = serde_json::json!([
            {"id": "call-1", "function": {"name": "read_file", "arguments": "{}"}},
            {"id": "call-1", "function": {"name": "read_file", "arguments": "{}"}}
        ]);
        let error = instrument_tool_calls(&calls).expect_err("duplicate call id must fail");
        assert!(error.contains("duplicate tool call id"));
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
