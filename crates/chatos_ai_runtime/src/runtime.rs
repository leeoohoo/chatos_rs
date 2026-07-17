// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::Value;

use chatos_mcp_runtime::ToolResult;
use tracing::{info, warn};

use crate::request::AiRequestHandler;
use crate::tool_call::tool_calls_value_has_items;
use crate::tool_runtime::append_tool_results_with_budget;
use crate::traits::{
    MemoryRecordWriter, ModelRequest, SaveAssistantRecordInput, SaveRecordInput,
    SaveToolRecordInput, ToolExecutor,
};
use crate::{
    RuntimeBeforeModelRequest, RuntimeFinalResponseAction, RuntimeFinalResponseContext,
    RuntimeIterationContext,
};

mod final_response;
mod input_items;
mod model_request;
mod options;
mod persistence;
mod report;
mod request_error;
mod summaries;
mod tool_execution;

pub use self::options::{AiRuntimeOptions, IterativeContextRefresh, MemoryContextOverflowRecovery};
pub use self::report::{AiRuntimeResult, AiTurnReport, AiTurnStatus};

use self::final_response::{
    handle_response_without_tool_calls, runtime_result_from_response, FinalResponseAction,
};
use self::input_items::{
    append_runtime_input_items, empty_final_response_followup_item, input_item_count,
    json_value_size_bytes, merge_pending_tool_turn_into_input,
};
use self::model_request::dispatch_model_request;
use self::persistence::{normalized_option, should_persist_tool_result};
use self::request_error::{handle_model_request_error, ModelRequestErrorAction};
use self::summaries::summarize_tool_call_names;
use self::tool_execution::{
    execute_runtime_tools, next_consecutive_failed_tool_batch_count, repeated_tool_failure_error,
};

pub struct AiRuntime {
    request_handler: AiRequestHandler,
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    record_writer: Option<Arc<dyn MemoryRecordWriter>>,
    max_iterations: usize,
}

const EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT: &str = "上一轮响应没有返回任何可展示的最终结果。请先检查当前任务是否已经真实完成：如果已经满足目标且不需要更多验证，直接输出最终结果；如果仍有未完成工作、未处理的任务状态/门禁反馈、缺少关键事实或缺少验证，请继续使用必要工具完成工作或记录明确阻塞。不要把未完成工作包装成最终结果。";
const EMPTY_FINAL_RESPONSE_ERROR: &str = "模型未返回可展示的最终结果";
const MAX_TRANSIENT_MODEL_REQUEST_RETRIES: usize = 5;
const MAX_CONSECUTIVE_FAILED_TOOL_BATCHES: usize = 8;

impl AiRuntime {
    pub fn builder() -> crate::builder::AiRuntimeBuilder {
        crate::builder::AiRuntimeBuilder::new()
    }

    pub fn new(tool_executor: Option<Arc<dyn ToolExecutor>>) -> Self {
        Self {
            request_handler: AiRequestHandler::new(),
            tool_executor,
            record_writer: None,
            max_iterations: 600,
        }
    }

    pub fn from_mcp_executor(executor: chatos_mcp_runtime::McpExecutor) -> Self {
        Self::new(Some(Arc::new(
            crate::mcp_executor::McpRuntimeToolExecutor::new(executor),
        )))
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    pub fn with_record_writer(
        mut self,
        record_writer: Option<Arc<dyn MemoryRecordWriter>>,
    ) -> Self {
        self.record_writer = record_writer;
        self
    }

    pub fn has_record_writer(&self) -> bool {
        self.record_writer.is_some()
    }

    pub async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        let Some(writer) = &self.record_writer else {
            return Ok(());
        };
        writer.save_record(input).await
    }

    pub async fn run_turn(
        &self,
        mut request: ModelRequest,
        options: AiRuntimeOptions,
    ) -> Result<AiRuntimeResult, String> {
        let mut iteration = 0usize;
        let mut context_overflow_recovery_attempted = false;
        let mut missing_tool_turn_replay_attempted = false;
        let mut iteration_reason = "initial".to_string();
        let mut pending_tool_calls: Option<Vec<Value>> = None;
        let mut pending_tool_outputs: Option<Vec<Value>> = None;
        let mut empty_final_response_followup_attempted = false;
        let mut runtime_followup_items: Vec<Value> = Vec::new();
        let mut runtime_followup_appended_to_request = false;
        let mut consecutive_failed_tool_batches = 0usize;
        'runtime_loop: loop {
            if options.is_aborted() {
                return Err("aborted".to_string());
            }
            if iteration >= self.max_iterations {
                warn!(
                    conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                    conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                    iteration,
                    max_iterations = self.max_iterations,
                    "ai runtime hit max iterations"
                );
                return Err("达到最大迭代次数".to_string());
            }
            iteration += 1;

            let mut input_rebuilt_for_iteration = false;
            if iteration > 1 {
                if let Some(refresh) = &options.iterative_context_refresh {
                    request.input = refresh.compose_input().await?;
                    request.input = merge_pending_tool_turn_into_input(
                        request.input,
                        pending_tool_calls.as_deref(),
                        pending_tool_outputs.as_deref(),
                    );
                    input_rebuilt_for_iteration = true;
                }
            }
            if !runtime_followup_items.is_empty()
                && (input_rebuilt_for_iteration || !runtime_followup_appended_to_request)
            {
                request.input =
                    append_runtime_input_items(request.input, runtime_followup_items.as_slice());
                if !input_rebuilt_for_iteration {
                    runtime_followup_appended_to_request = true;
                }
            }

            if let Some(executor) = &self.tool_executor {
                let tools = executor.available_tools();
                if !tools.is_empty() {
                    request.tools = tools;
                }
            }

            let (iteration_request, lifecycle_before) =
                prepare_iteration_request(&request, &options, iteration, iteration_reason.as_str())
                    .await?;

            let input_item_count = input_item_count(&iteration_request.input);
            let input_bytes = json_value_size_bytes(&iteration_request.input);
            let tool_count = iteration_request.tools.len();
            let mut transient_retry_count = 0usize;
            let mut response = loop {
                let response = dispatch_model_request(
                    &self.request_handler,
                    &iteration_request,
                    &options,
                    iteration,
                    iteration_reason.as_str(),
                    input_item_count,
                    input_bytes,
                    tool_count,
                    lifecycle_before.stream_output,
                )
                .await;
                match response {
                    Ok(response) => break response,
                    Err(err) => {
                        match handle_model_request_error(
                            err,
                            &iteration_request,
                            &options,
                            iteration,
                            missing_tool_turn_replay_attempted,
                            pending_tool_calls.as_deref(),
                            pending_tool_outputs.as_deref(),
                            &mut context_overflow_recovery_attempted,
                            &mut transient_retry_count,
                        )
                        .await?
                        {
                            ModelRequestErrorAction::ReplayMissingToolTurn(repaired_input) => {
                                request.input = repaired_input;
                                missing_tool_turn_replay_attempted = true;
                                iteration_reason = "missing_tool_turn_replay".to_string();
                                continue 'runtime_loop;
                            }
                            ModelRequestErrorAction::ContextRecovered => {
                                iteration_reason = "context_overflow_recovery".to_string();
                                continue 'runtime_loop;
                            }
                            ModelRequestErrorAction::RetryRequest => continue,
                            ModelRequestErrorAction::Fail(err) => return Err(err),
                        }
                    }
                }
            };
            missing_tool_turn_replay_attempted = false;

            if options.is_aborted() {
                return Err("aborted".to_string());
            }

            let Some(tool_calls) = response
                .tool_calls
                .clone()
                .filter(|value| tool_calls_value_has_items(Some(value)))
            else {
                match handle_response_without_tool_calls(
                    &response,
                    &options,
                    iteration,
                    self.max_iterations,
                    empty_final_response_followup_attempted,
                )? {
                    FinalResponseAction::AskForFollowup => {
                        empty_final_response_followup_attempted = true;
                        runtime_followup_items = vec![empty_final_response_followup_item()];
                        runtime_followup_appended_to_request = false;
                        iteration_reason = "empty_final_response_followup".to_string();
                        continue;
                    }
                    FinalResponseAction::Complete => {
                        if let Some(hook) = &options.lifecycle_hook {
                            match hook
                                .after_final_response(RuntimeFinalResponseContext {
                                    conversation_id: options.conversation_id.clone(),
                                    conversation_turn_id: options.conversation_turn_id.clone(),
                                    iteration,
                                    reason: iteration_reason.clone(),
                                    response: response.clone(),
                                })
                                .await?
                            {
                                RuntimeFinalResponseAction::Accept => {}
                                RuntimeFinalResponseAction::Replace(replacement) => {
                                    response = *replacement;
                                }
                                RuntimeFinalResponseAction::Continue {
                                    input_items,
                                    reason,
                                } => {
                                    runtime_followup_items = input_items;
                                    runtime_followup_appended_to_request = false;
                                    iteration_reason = if reason.trim().is_empty() {
                                        "lifecycle_followup".to_string()
                                    } else {
                                        reason
                                    };
                                    if let Some(callback) = &options.callbacks.on_turn_phase {
                                        callback(serde_json::json!({
                                            "phase": "continue",
                                            "reason": iteration_reason,
                                            "iteration": iteration,
                                        }));
                                    }
                                    continue;
                                }
                            }
                        }
                        let lifecycle_metadata = if let Some(hook) = &options.lifecycle_hook {
                            hook.final_response_metadata(RuntimeFinalResponseContext {
                                conversation_id: options.conversation_id.clone(),
                                conversation_turn_id: options.conversation_turn_id.clone(),
                                iteration,
                                reason: iteration_reason.clone(),
                                response: response.clone(),
                            })
                            .await?
                        } else {
                            None
                        };
                        self.save_assistant_record(
                            &options,
                            &response,
                            response.tool_calls.clone(),
                            None,
                            lifecycle_metadata,
                        )
                        .await?;
                        return Ok(runtime_result_from_response(response));
                    }
                }
            };

            let tool_call_count = tool_calls.as_array().map(Vec::len).unwrap_or_default();
            let tool_names = summarize_tool_call_names(&tool_calls, 8);
            info!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                tool_call_count,
                tool_names = tool_names.join(", "),
                "ai runtime received tool calls and will continue loop"
            );
            self.save_assistant_record(
                &options,
                &response,
                Some(tool_calls.clone()),
                Some("tool_calls".to_string()),
                None,
            )
            .await?;

            let Some(executor) = &self.tool_executor else {
                return Ok(runtime_result_from_response(response));
            };

            let tool_execution =
                execute_runtime_tools(executor.as_ref(), &tool_calls, &options, iteration).await?;
            self.save_tool_records(&options, tool_execution.tool_results.as_slice())
                .await?;
            consecutive_failed_tool_batches = next_consecutive_failed_tool_batch_count(
                consecutive_failed_tool_batches,
                tool_execution.tool_results.as_slice(),
            );
            if consecutive_failed_tool_batches >= MAX_CONSECUTIVE_FAILED_TOOL_BATCHES {
                let error = repeated_tool_failure_error(
                    tool_execution.tool_results.as_slice(),
                    consecutive_failed_tool_batches,
                );
                warn!(
                    conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                    conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                    iteration,
                    consecutive_failed_tool_batches,
                    "ai runtime stopped after repeated failed tool batches"
                );
                return Err(error);
            }
            pending_tool_calls = Some(tool_execution.tool_call_items);
            pending_tool_outputs = Some(tool_execution.tool_output_items);
            if options.iterative_context_refresh.is_none() {
                request.input = append_tool_results_with_budget(
                    request.input,
                    request.supports_responses,
                    &response.content,
                    &tool_calls,
                    tool_execution.tool_results,
                    options.tool_result_model_budget_limits,
                );
            }
            iteration_reason = "tool_results".to_string();
        }
    }

    async fn save_assistant_record(
        &self,
        options: &AiRuntimeOptions,
        response: &crate::request::AiResponse,
        tool_calls: Option<Value>,
        response_status: Option<String>,
        metadata_override: Option<Value>,
    ) -> Result<(), String> {
        if !options.record_options.persist_assistant_records {
            return Ok(());
        }
        let Some(writer) = &self.record_writer else {
            return Ok(());
        };
        let Some(conversation_id) = normalized_option(options.conversation_id.as_deref()) else {
            return Ok(());
        };
        writer
            .save_assistant_record(SaveAssistantRecordInput {
                conversation_id,
                conversation_turn_id: options.conversation_turn_id.clone(),
                message_id: None,
                content: response.content.clone(),
                reasoning: response.reasoning.clone(),
                structured_payload: tool_calls
                    .clone()
                    .filter(|value| tool_calls_value_has_items(Some(value))),
                metadata: merge_record_metadata(
                    options.record_options.assistant_metadata.clone(),
                    metadata_override,
                ),
                tool_calls,
                response_id: response.response_id.clone(),
                response_status: response_status.or_else(|| response.finish_reason.clone()),
                message_mode: options.record_options.assistant_message_mode.clone(),
                message_source: options.record_options.assistant_message_source.clone(),
                summary_status: None,
                summary_id: None,
                summarized_at: None,
                created_at: None,
            })
            .await
    }

    async fn save_tool_records(
        &self,
        options: &AiRuntimeOptions,
        tool_results: &[ToolResult],
    ) -> Result<(), String> {
        if !options.record_options.persist_tool_records || tool_results.is_empty() {
            return Ok(());
        }
        let Some(writer) = &self.record_writer else {
            return Ok(());
        };
        let Some(conversation_id) = normalized_option(options.conversation_id.as_deref()) else {
            return Ok(());
        };
        let records = tool_results
            .iter()
            .filter(|result| should_persist_tool_result(result))
            .map(|result| {
                let mut input = SaveToolRecordInput::from_tool_result(
                    conversation_id.clone(),
                    options.conversation_turn_id.clone(),
                    result,
                );
                input.metadata = options.record_options.tool_metadata.clone();
                input.message_mode = options.record_options.tool_message_mode.clone();
                input.message_source = options.record_options.tool_message_source.clone();
                input
            })
            .collect::<Vec<_>>();
        if records.is_empty() {
            return Ok(());
        }
        writer.save_tool_records(records).await
    }
}

fn merge_record_metadata(base: Option<Value>, overlay: Option<Value>) -> Option<Value> {
    match (base, overlay) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(Value::Object(mut base)), Some(Value::Object(overlay))) => {
            base.extend(overlay);
            Some(Value::Object(base))
        }
        (_, Some(overlay)) => Some(overlay),
    }
}

async fn prepare_iteration_request(
    request: &ModelRequest,
    options: &AiRuntimeOptions,
    iteration: usize,
    iteration_reason: &str,
) -> Result<(ModelRequest, RuntimeBeforeModelRequest), String> {
    let lifecycle_before = if let Some(hook) = &options.lifecycle_hook {
        hook.before_model_request(RuntimeIterationContext {
            conversation_id: options.conversation_id.clone(),
            conversation_turn_id: options.conversation_turn_id.clone(),
            iteration,
            reason: iteration_reason.to_string(),
            input: request.input.clone(),
        })
        .await?
    } else {
        RuntimeBeforeModelRequest::unchanged()
    };
    let mut iteration_request = request.clone();
    if !lifecycle_before.input_items.is_empty() {
        iteration_request.input = append_runtime_input_items(
            iteration_request.input,
            lifecycle_before.input_items.as_slice(),
        );
    }
    if !lifecycle_before.tools_enabled {
        iteration_request.tools.clear();
    }
    Ok((iteration_request, lifecycle_before))
}

#[cfg(test)]
mod tests;
