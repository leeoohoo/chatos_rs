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

    let tool_result_count = tool_results.len();
    let tool_result_names = summarize_tool_result_names(tool_results.as_slice(), 8);
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
        tool_result_names = tool_result_names.join(", "),
        tool_batch_ms = started_at.elapsed().as_millis(),
        "ai runtime finished tool execution"
    );

    Ok(RuntimeToolExecution {
        tool_results,
        tool_call_items,
        tool_output_items,
    })
}
