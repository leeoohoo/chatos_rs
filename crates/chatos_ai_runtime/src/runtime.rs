// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::Value;

use chatos_mcp_runtime::ToolResult;
#[cfg(feature = "local-agent-loop")]
use tracing::{info, warn};

#[cfg(feature = "local-agent-loop")]
use crate::error_policy::is_missing_tool_call_error;
#[cfg(feature = "local-agent-loop")]
use crate::file_write_recovery::automatic_file_write_recovery_calls;
use crate::request::AiRequestHandler;
#[cfg(feature = "local-agent-loop")]
use crate::request_retry::is_previous_response_id_unsupported_error;
use crate::tool_call::tool_calls_value_has_items;
#[cfg(feature = "local-agent-loop")]
use crate::tool_runtime::append_tool_results_with_budget;
use crate::traits::SaveToolRecordInput;
use crate::traits::{
    MemoryRecordWriter, ModelRequest, SaveAssistantRecordInput, SaveRecordInput, ToolExecutor,
};
use crate::{RuntimeBeforeModelRequest, RuntimeIterationContext};
#[cfg(feature = "local-agent-loop")]
use crate::{RuntimeFinalResponseAction, RuntimeFinalResponseContext};

mod final_response;
mod input_items;
mod model_request;
mod options;
mod persistence;
mod report;
mod request_error;
mod single_step;
#[cfg(feature = "local-agent-loop")]
mod summaries;
#[cfg(feature = "local-agent-loop")]
mod tool_execution;

pub use self::options::{AiRuntimeOptions, IterativeContextRefresh, MemoryContextOverflowRecovery};
pub use self::report::{AiRuntimeResult, AiTurnReport, AiTurnStatus};
pub use self::single_step::{AiSingleStepOutcome, AiSingleStepRequest};

use self::final_response::runtime_result_from_response;
#[cfg(feature = "local-agent-loop")]
use self::final_response::{handle_response_without_tool_calls, FinalResponseAction};
use self::input_items::append_runtime_input_items;
#[cfg(feature = "local-agent-loop")]
use self::input_items::empty_final_response_followup_item;
#[cfg(feature = "local-agent-loop")]
use self::input_items::{
    input_item_count, json_value_size_bytes, merge_current_turn_tool_history_into_input,
    merge_pending_tool_turn_into_input,
};
#[cfg(feature = "local-agent-loop")]
use self::model_request::dispatch_model_request;
use self::persistence::normalized_option;
use self::persistence::should_persist_tool_result;
use self::request_error::downgraded_thinking_level;
#[cfg(feature = "local-agent-loop")]
use self::request_error::{handle_model_request_error, ModelRequestErrorAction};
#[cfg(feature = "local-agent-loop")]
use self::summaries::summarize_tool_call_names;
#[cfg(feature = "local-agent-loop")]
use self::tool_execution::{
    execute_runtime_tools, next_consecutive_failed_tool_batch_count, repeated_tool_failure_error,
};

pub struct AiRuntime {
    request_handler: AiRequestHandler,
    #[cfg_attr(not(feature = "local-agent-loop"), allow(dead_code))]
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    record_writer: Option<Arc<dyn MemoryRecordWriter>>,
    max_iterations: usize,
}

const EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT: &str = "上一轮响应没有返回任何可展示的最终结果。请先检查当前任务是否已经真实完成：如果已经满足目标且不需要更多验证，直接输出最终结果；如果仍有未完成工作、未处理的任务状态/门禁反馈、缺少关键事实或缺少验证，请继续使用必要工具完成工作或记录明确阻塞。不要把未完成工作包装成最终结果。";
#[cfg(feature = "local-agent-loop")]
const EMPTY_FINAL_RESPONSE_ERROR: &str = "模型未返回可展示的最终结果";
#[cfg(feature = "local-agent-loop")]
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

    /// Persists a tool batch executed by an external event-driven MCP service.
    /// The local approval Agent still uses the in-process loop; cloud Agents
    /// call this after the aggregate MCP result arrives.
    pub async fn persist_external_tool_results(
        &self,
        options: &AiRuntimeOptions,
        tool_results: &[ToolResult],
    ) -> Result<(), String> {
        self.save_tool_records(options, tool_results).await
    }

    /// Executes one model request and returns any tool work or retry as data.
    /// Cloud consumers persist that outcome and end the current MQ delivery.
    pub async fn execute_once(
        &self,
        request: AiSingleStepRequest,
    ) -> Result<AiSingleStepOutcome, String> {
        single_step::execute_once(self, request).await
    }

    #[cfg(feature = "local-agent-loop")]
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
        let mut current_turn_tool_calls = Vec::new();
        let mut current_turn_tool_outputs = Vec::new();
        let mut empty_final_response_followup_attempted = false;
        let mut runtime_followup_items: Vec<Value> = Vec::new();
        let mut runtime_followup_appended_to_request = false;
        let mut consecutive_failed_tool_batches = 0usize;
        let mut continuation_input = request
            .previous_response_id
            .as_ref()
            .map(|_| request.input.clone());
        let mut continuation_disabled = false;
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
                    request.previous_response_id = None;
                    continuation_input = None;
                    request.input = refresh.compose_input().await?;
                    request.input = merge_current_turn_tool_history_into_input(
                        request.input,
                        current_turn_tool_calls.as_slice(),
                        current_turn_tool_outputs.as_slice(),
                        options.tool_result_model_budget_limits,
                    );
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

            let (mut iteration_request, lifecycle_before) =
                prepare_iteration_request(&request, &options, iteration, iteration_reason.as_str())
                    .await?;
            let standalone_iteration_input = iteration_request.input.clone();
            if request.supports_responses
                && options.iterative_context_refresh.is_none()
                && !continuation_disabled
            {
                if let (Some(previous_response_id), Some(delta_input)) = (
                    request.previous_response_id.clone(),
                    continuation_input.clone(),
                ) {
                    iteration_request.previous_response_id = Some(previous_response_id);
                    iteration_request.input = append_runtime_input_items(
                        delta_input,
                        lifecycle_before.input_items.as_slice(),
                    );
                }
            } else {
                iteration_request.previous_response_id = None;
            }

            let tool_count = iteration_request.tools.len();
            let mut transient_retry_count = 0usize;
            let mut recovery_request_handler: Option<AiRequestHandler> = None;
            let mut force_non_stream = false;
            let mut request_attempt = 0usize;
            let mut response = loop {
                request_attempt = request_attempt.saturating_add(1);
                let request_handler = recovery_request_handler
                    .as_ref()
                    .unwrap_or(&self.request_handler);
                let force_identity_encoding = recovery_request_handler.is_some();
                let provider_stream = !force_non_stream;
                let input_item_count = input_item_count(&iteration_request.input);
                let input_bytes = json_value_size_bytes(&iteration_request.input);
                let response = dispatch_model_request(
                    request_handler,
                    &iteration_request,
                    &options,
                    iteration,
                    iteration_reason.as_str(),
                    input_item_count,
                    input_bytes,
                    tool_count,
                    request_attempt,
                    lifecycle_before.stream_output,
                    provider_stream,
                    force_identity_encoding,
                )
                .await;
                match response {
                    Ok(response) => break response,
                    Err(err) => {
                        if iteration_request.previous_response_id.is_some()
                            && (is_previous_response_id_unsupported_error(err.as_str())
                                || is_missing_tool_call_error(err.as_str()))
                        {
                            warn!(
                                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                                conversation_turn_id = options
                                    .conversation_turn_id
                                    .as_deref()
                                    .unwrap_or(""),
                                iteration,
                                "provider rejected Responses continuation; retrying with standalone input"
                            );
                            iteration_request.previous_response_id = None;
                            iteration_request.input = standalone_iteration_input.clone();
                            request.previous_response_id = None;
                            continuation_input = None;
                            continuation_disabled = true;
                            continue;
                        }
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
                            provider_stream,
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
                            ModelRequestErrorAction::RetryRequest {
                                disable_stream,
                                downgrade_thinking,
                            } => {
                                // A retry must not inherit a potentially unhealthy pooled
                                // connection. Build a new client for every retry attempt and
                                // ask the provider to close that isolated connection afterward.
                                force_non_stream |= disable_stream;
                                if downgrade_thinking {
                                    if let Some(next_level) = downgraded_thinking_level(
                                        iteration_request.thinking_level.as_deref(),
                                    ) {
                                        warn!(
                                            conversation_id = options
                                                .conversation_id
                                                .as_deref()
                                                .unwrap_or(""),
                                            conversation_turn_id = options
                                                .conversation_turn_id
                                                .as_deref()
                                                .unwrap_or(""),
                                            iteration,
                                            request_attempt,
                                            previous_thinking_level = iteration_request
                                                .thinking_level
                                                .as_deref()
                                                .unwrap_or("default"),
                                            next_thinking_level = next_level.as_str(),
                                            "ai runtime lowering reasoning effort for transport retry"
                                        );
                                        iteration_request.thinking_level = Some(next_level);
                                    }
                                }
                                recovery_request_handler = Some(AiRequestHandler::new());
                                continue;
                            }
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
                        set_next_continuation(
                            &mut request,
                            &mut continuation_input,
                            continuation_disabled,
                            &options,
                            response.response_id.as_deref(),
                            runtime_followup_items.as_slice(),
                        );
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
                                    set_next_continuation(
                                        &mut request,
                                        &mut continuation_input,
                                        continuation_disabled,
                                        &options,
                                        response.response_id.as_deref(),
                                        runtime_followup_items.as_slice(),
                                    );
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

            let mut tool_execution =
                execute_runtime_tools(executor.as_ref(), &tool_calls, &options, iteration).await?;
            let provider_tool_call_count = tool_execution.tool_call_items.len();
            self.save_tool_records(&options, tool_execution.tool_results.as_slice())
                .await?;
            let recovery_calls = automatic_file_write_recovery_calls(
                tool_execution.tool_results.as_slice(),
                executor.available_tools().as_slice(),
            )?;
            if !recovery_calls.is_empty() {
                info!(
                    conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                    conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                    iteration,
                    recovery_call_count = recovery_calls.len(),
                    "ai runtime automatically re-reading stale modification targets"
                );
                let recovery_execution = execute_runtime_tools(
                    executor.as_ref(),
                    &Value::Array(recovery_calls),
                    &options,
                    iteration,
                )
                .await?;
                self.save_tool_records(&options, recovery_execution.tool_results.as_slice())
                    .await?;
                tool_execution.extend(recovery_execution);
            }
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
            if options.iterative_context_refresh.is_some() {
                current_turn_tool_calls.extend(tool_execution.tool_call_items.iter().cloned());
                current_turn_tool_outputs.extend(
                    tool_execution
                        .tool_output_items
                        .iter()
                        .filter(|item| {
                            item.get("type").and_then(Value::as_str) == Some("function_call_output")
                        })
                        .cloned(),
                );
            }
            let executed_tool_calls = Value::Array(tool_execution.tool_calls.clone());
            let continuation_tool_items = continuation_tool_input_items(
                tool_execution.tool_call_items.as_slice(),
                tool_execution.tool_output_items.as_slice(),
                provider_tool_call_count,
            );
            pending_tool_calls = Some(tool_execution.tool_call_items);
            pending_tool_outputs = Some(tool_execution.tool_output_items);
            if options.iterative_context_refresh.is_none() {
                request.input = append_tool_results_with_budget(
                    request.input,
                    request.supports_responses,
                    &response.content,
                    &executed_tool_calls,
                    tool_execution.tool_results,
                    options.tool_result_model_budget_limits,
                );
                set_next_continuation(
                    &mut request,
                    &mut continuation_input,
                    continuation_disabled,
                    &options,
                    response.response_id.as_deref(),
                    continuation_tool_items.as_slice(),
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
                message_id: options.record_options.assistant_message_id.clone(),
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
            .enumerate()
            .map(|(index, result)| {
                let mut input = SaveToolRecordInput::from_tool_result(
                    conversation_id.clone(),
                    options.conversation_turn_id.clone(),
                    result,
                );
                input.message_id = options
                    .record_options
                    .tool_message_id_prefix
                    .as_ref()
                    .map(|prefix| format!("{prefix}:{index}"));
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

#[cfg(feature = "local-agent-loop")]
fn set_next_continuation(
    request: &mut ModelRequest,
    continuation_input: &mut Option<Value>,
    continuation_disabled: bool,
    options: &AiRuntimeOptions,
    response_id: Option<&str>,
    input_items: &[Value],
) {
    let response_id = normalized_option(response_id);
    if request.supports_responses
        && options.iterative_context_refresh.is_none()
        && !continuation_disabled
        && response_id.is_some()
        && !input_items.is_empty()
    {
        request.previous_response_id = response_id;
        *continuation_input = Some(Value::Array(input_items.to_vec()));
    } else {
        request.previous_response_id = None;
        *continuation_input = None;
    }
}

#[cfg(feature = "local-agent-loop")]
fn continuation_tool_input_items(
    tool_call_items: &[Value],
    tool_output_items: &[Value],
    provider_tool_call_count: usize,
) -> Vec<Value> {
    let provider_tool_call_count = provider_tool_call_count
        .min(tool_call_items.len())
        .min(tool_output_items.len());
    let mut items = tool_output_items[..provider_tool_call_count].to_vec();

    // Runtime-generated recovery calls are not present in the provider's previous
    // response, so their call items must accompany their outputs.
    items.extend_from_slice(&tool_call_items[provider_tool_call_count..]);
    items.extend_from_slice(&tool_output_items[provider_tool_call_count..]);
    items
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
    } else if !lifecycle_before.disabled_tool_names.is_empty() {
        iteration_request.tools.retain(|tool| {
            runtime_tool_definition_name(tool).is_none_or(|name| {
                !lifecycle_before
                    .disabled_tool_names
                    .iter()
                    .any(|disabled| disabled == name)
            })
        });
    }
    if let Some(output_format) = lifecycle_before.output_format.clone() {
        iteration_request.output_format = Some(output_format);
    }
    Ok((iteration_request, lifecycle_before))
}

fn runtime_tool_definition_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(Value::as_str)
        .or_else(|| tool.get("function")?.get("name")?.as_str())
}

#[cfg(all(test, feature = "local-agent-loop"))]
mod tests;
