use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use chatos_mcp_runtime::{
    ToolAbortCheckCallback, ToolCallContext, ToolCallerModelRuntime, ToolResult, ToolResultCallback,
};
use tracing::{info, warn};

use crate::error_policy::{
    is_context_length_exceeded_error, is_missing_tool_call_error, is_request_body_too_large_error,
};
use crate::memory_context::{MemoryContextComposer, MemoryScope};
use crate::request::{AiRequestHandler, AiRequestOptions, StreamCallbacks};
use crate::tool_call::{extract_tool_call_name, tool_calls_value_has_items};
use crate::tool_runtime::{
    append_tool_results_with_budget, build_tool_call_items, build_tool_output_items_with_budget,
    merge_pending_tool_turn_items, ToolResultModelBudgetLimits,
};
use crate::traits::{
    MemoryRecordWriter, ModelRequest, RuntimeCallbacks, RuntimeRecordOptions,
    SaveAssistantRecordInput, SaveRecordInput, SaveToolRecordInput, ToolExecutor,
};

pub struct AiRuntime {
    request_handler: AiRequestHandler,
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    record_writer: Option<Arc<dyn MemoryRecordWriter>>,
    max_iterations: usize,
}

const EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT: &str = "上一轮响应没有返回任何可展示的最终结果。请不要继续调用工具，直接基于目前对话上下文、已执行步骤和已有工具结果，输出本次任务的最终结果；如果无法完成，请明确说明阻塞原因、已完成事项和下一步建议。";
const EMPTY_FINAL_RESPONSE_ERROR: &str = "模型未返回可展示的最终结果";

#[derive(Clone)]
pub struct AiRuntimeOptions {
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub caller_model: Option<String>,
    pub caller_model_runtime: Option<ToolCallerModelRuntime>,
    pub abort_checker: Option<ToolAbortCheckCallback>,
    pub tool_result_model_budget_limits: Option<ToolResultModelBudgetLimits>,
    pub callbacks: RuntimeCallbacks,
    pub record_options: RuntimeRecordOptions,
    pub iterative_context_refresh: Option<IterativeContextRefresh>,
}

#[derive(Clone)]
pub struct IterativeContextRefresh {
    memory_composer: Option<MemoryContextComposer>,
    memory_scope: Option<MemoryScope>,
    prefixed_input_items: Vec<Value>,
    sticky_input_items: Vec<Value>,
    tool_result_model_budget_limits: Option<ToolResultModelBudgetLimits>,
    context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
}

#[derive(Clone)]
pub struct MemoryContextOverflowRecovery {
    poll_interval: Duration,
    poll_timeout: Duration,
    trigger_reason: Option<String>,
}

impl IterativeContextRefresh {
    pub fn new(
        memory_composer: Option<MemoryContextComposer>,
        memory_scope: Option<MemoryScope>,
        prefixed_input_items: Vec<Value>,
    ) -> Self {
        Self {
            memory_composer,
            memory_scope,
            prefixed_input_items,
            sticky_input_items: Vec::new(),
            tool_result_model_budget_limits: None,
            context_overflow_recovery: None,
        }
    }

    pub fn with_sticky_input_items(mut self, sticky_input_items: Vec<Value>) -> Self {
        self.sticky_input_items = sticky_input_items;
        self
    }

    pub fn with_tool_result_model_budget_limits(
        mut self,
        limits: Option<ToolResultModelBudgetLimits>,
    ) -> Self {
        self.tool_result_model_budget_limits = limits;
        self
    }

    pub fn with_context_overflow_recovery(
        mut self,
        context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
    ) -> Self {
        self.context_overflow_recovery = context_overflow_recovery;
        self
    }

    pub async fn compose_input(&self) -> Result<Value, String> {
        let mut items = Vec::new();
        items.extend(self.prefixed_input_items.iter().cloned());

        if let (Some(composer), Some(scope)) =
            (self.memory_composer.as_ref(), self.memory_scope.as_ref())
        {
            items.extend(
                composer
                    .compose_input_items_with_budget(scope, self.tool_result_model_budget_limits)
                    .await?,
            );
        }

        items.extend(self.sticky_input_items.iter().cloned());
        Ok(Value::Array(items))
    }

    pub async fn try_recover_from_context_overflow(
        &self,
        callbacks: &RuntimeCallbacks,
    ) -> Result<bool, String> {
        let Some(recovery) = &self.context_overflow_recovery else {
            return Ok(false);
        };
        let (Some(composer), Some(scope)) =
            (self.memory_composer.as_ref(), self.memory_scope.as_ref())
        else {
            return Ok(false);
        };

        notify_context_overflow_recovery(
            callbacks,
            "正在自动压缩上下文，压缩完成后将继续当前请求。",
        );
        let initial = composer
            .run_active_summary(scope, recovery.trigger_reason.as_deref())
            .await?;
        let status = composer
            .wait_for_active_summary_completion(
                scope,
                initial,
                recovery.poll_interval,
                recovery.poll_timeout,
            )
            .await?;
        if status.failed || (!status.generated && !status.compacted) {
            return Ok(false);
        }

        notify_context_overflow_recovery(callbacks, "上下文压缩完成，正在继续当前请求。");
        Ok(true)
    }
}

impl MemoryContextOverflowRecovery {
    pub fn new() -> Self {
        Self {
            poll_interval: Duration::from_secs(10),
            poll_timeout: Duration::from_secs(120),
            trigger_reason: Some("context_overflow".to_string()),
        }
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub fn with_poll_timeout(mut self, poll_timeout: Duration) -> Self {
        self.poll_timeout = poll_timeout;
        self
    }

    pub fn with_trigger_reason(mut self, trigger_reason: impl Into<String>) -> Self {
        self.trigger_reason = Some(trigger_reason.into());
        self
    }

    pub fn with_optional_trigger_reason(mut self, trigger_reason: Option<String>) -> Self {
        self.trigger_reason = trigger_reason;
        self
    }
}

impl AiRuntimeOptions {
    pub fn new(conversation_id: Option<String>, conversation_turn_id: Option<String>) -> Self {
        Self {
            conversation_id,
            conversation_turn_id,
            caller_model: None,
            caller_model_runtime: None,
            abort_checker: None,
            tool_result_model_budget_limits: None,
            callbacks: RuntimeCallbacks::default(),
            record_options: RuntimeRecordOptions::default(),
            iterative_context_refresh: None,
        }
    }

    pub fn for_conversation(conversation_id: impl Into<String>) -> Self {
        Self::new(Some(conversation_id.into()), None)
    }

    pub fn with_conversation_turn_id(mut self, conversation_turn_id: impl Into<String>) -> Self {
        self.conversation_turn_id = Some(conversation_turn_id.into());
        self
    }

    pub fn with_caller_model(mut self, caller_model: Option<String>) -> Self {
        self.caller_model = caller_model;
        self
    }

    pub fn with_caller_model_runtime(
        mut self,
        caller_model_runtime: Option<ToolCallerModelRuntime>,
    ) -> Self {
        if self.caller_model.is_none() {
            self.caller_model = caller_model_runtime
                .as_ref()
                .map(|runtime| runtime.model.clone())
                .filter(|model| !model.trim().is_empty());
        }
        self.caller_model_runtime = caller_model_runtime;
        self
    }

    pub fn with_abort_checker(mut self, abort_checker: Option<ToolAbortCheckCallback>) -> Self {
        self.abort_checker = abort_checker;
        self
    }

    pub fn with_tool_result_model_budget_limits(
        mut self,
        limits: Option<ToolResultModelBudgetLimits>,
    ) -> Self {
        self.tool_result_model_budget_limits = limits;
        self
    }

    pub fn with_callbacks(mut self, callbacks: RuntimeCallbacks) -> Self {
        self.callbacks = callbacks;
        self
    }

    pub fn with_record_options(mut self, record_options: RuntimeRecordOptions) -> Self {
        self.record_options = record_options;
        self
    }

    pub fn with_iterative_context_refresh(
        mut self,
        iterative_context_refresh: Option<IterativeContextRefresh>,
    ) -> Self {
        self.iterative_context_refresh = iterative_context_refresh;
        self
    }

    pub fn is_aborted(&self) -> bool {
        let Some(conversation_id) = self.conversation_id.as_deref() else {
            return false;
        };
        self.abort_checker
            .as_ref()
            .is_some_and(|callback| callback(conversation_id))
    }

    pub fn tool_call_context(&self) -> ToolCallContext {
        let context = ToolCallContext::new(
            self.conversation_id.clone(),
            self.conversation_turn_id.clone(),
            self.caller_model.clone(),
        )
        .with_caller_model_runtime(self.caller_model_runtime.clone());
        if let Some(abort_checker) = &self.abort_checker {
            context.with_abort_checker(Arc::clone(abort_checker))
        } else {
            context
        }
    }
}

impl Default for AiRuntimeOptions {
    fn default() -> Self {
        Self::new(None, None)
    }
}

#[derive(Debug, Clone)]
pub struct AiRuntimeResult {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
}

impl AiRuntimeResult {
    pub fn into_report(self) -> AiTurnReport {
        AiTurnReport::completed(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTurnStatus {
    Completed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTurnReport {
    pub status: AiTurnStatus,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub error: Option<String>,
    pub tool_calls: Option<Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    pub completed_at: String,
}

impl AiTurnReport {
    pub fn completed(result: AiRuntimeResult) -> Self {
        Self {
            status: AiTurnStatus::Completed,
            content: Some(result.content),
            reasoning: result.reasoning,
            error: None,
            tool_calls: result.tool_calls,
            finish_reason: result.finish_reason,
            usage: result.usage,
            response_id: result.response_id,
            completed_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        let error = error.into();
        let status = if error == "aborted" {
            AiTurnStatus::Aborted
        } else {
            AiTurnStatus::Failed
        };
        Self {
            status,
            content: None,
            reasoning: None,
            error: Some(error),
            tool_calls: None,
            finish_reason: None,
            usage: None,
            response_id: None,
            completed_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn aborted() -> Self {
        Self::failed("aborted")
    }

    pub fn is_completed(&self) -> bool {
        self.status == AiTurnStatus::Completed
    }

    pub fn is_aborted(&self) -> bool {
        self.status == AiTurnStatus::Aborted
    }

    pub fn user_message(&self) -> String {
        match self.status {
            AiTurnStatus::Completed => {
                if let Some(content) = self
                    .content
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    format!("任务已完成。\n\n{content}")
                } else {
                    "任务已完成。".to_string()
                }
            }
            AiTurnStatus::Failed => {
                let error = self
                    .error
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("未知错误");
                format!("任务执行失败：{error}")
            }
            AiTurnStatus::Aborted => "任务已取消。".to_string(),
        }
    }
}

impl AiRuntime {
    pub fn builder() -> crate::builder::AiRuntimeBuilder {
        crate::builder::AiRuntimeBuilder::new()
    }

    pub fn new(tool_executor: Option<Arc<dyn ToolExecutor>>) -> Self {
        Self {
            request_handler: AiRequestHandler::new(),
            tool_executor,
            record_writer: None,
            max_iterations: 25,
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
        loop {
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

            let input_item_count = input_item_count(&request.input);
            let input_bytes = json_value_size_bytes(&request.input);
            let tool_count = request.tools.len();
            info!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                reason = iteration_reason.as_str(),
                model = request.model.as_str(),
                provider = request.provider.as_str(),
                supports_responses = request.supports_responses,
                input_item_count,
                input_bytes,
                tool_count,
                "ai runtime dispatching model request"
            );
            let request_debug = json!({
                "conversation_id": options.conversation_id.clone(),
                "conversation_turn_id": options.conversation_turn_id.clone(),
                "iteration": iteration,
                "reason": iteration_reason.clone(),
                "input_item_count": input_item_count,
                "input_bytes": input_bytes,
                "tool_count": tool_count,
                "supports_responses": request.supports_responses,
            });
            let on_before_model_request =
                options
                    .callbacks
                    .on_before_model_request
                    .as_ref()
                    .map(|cb| {
                        let cb = Arc::clone(cb);
                        let request_debug = request_debug.clone();
                        Arc::new(move |payload: Value| {
                            cb(attach_runtime_debug(payload, &request_debug));
                        }) as Arc<dyn Fn(Value) + Send + Sync>
                    });
            let response = self
                .request_handler
                .handle_request_with_options(
                    request.base_url.as_str(),
                    request.api_key.as_str(),
                    request.input.clone(),
                    request.supports_responses,
                    request.model.clone(),
                    request.instructions.clone(),
                    Some(request.tools.clone()),
                    request.temperature,
                    request.max_output_tokens,
                    StreamCallbacks {
                        on_chunk: options.callbacks.on_chunk.clone(),
                        on_thinking: options.callbacks.on_thinking.clone(),
                    },
                    Some(request.provider.clone()),
                    request.thinking_level.clone(),
                    on_before_model_request,
                    AiRequestOptions {
                        prompt_cache_key: request.prompt_cache_key.clone(),
                        request_cwd: request.request_cwd.clone(),
                        include_prompt_cache_retention: request.include_prompt_cache_retention,
                        request_body_limit_bytes: request.request_body_limit_bytes,
                        abort_token: None,
                        force_identity_encoding: false,
                    },
                )
                .await;
            let response = match response {
                Ok(response) => response,
                Err(err) => {
                    if !missing_tool_turn_replay_attempted
                        && request.supports_responses
                        && is_missing_tool_call_error(err.as_str())
                    {
                        let repaired_input = merge_pending_tool_turn_into_input(
                            request.input.clone(),
                            pending_tool_calls.as_deref(),
                            pending_tool_outputs.as_deref(),
                        );
                        if repaired_input != request.input {
                            warn!(
                                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                                conversation_turn_id =
                                    options.conversation_turn_id.as_deref().unwrap_or(""),
                                iteration,
                                error = err.as_str(),
                                "ai runtime replaying pending tool turn after provider rejected incomplete tool exchange"
                            );
                            request.input = repaired_input;
                            missing_tool_turn_replay_attempted = true;
                            iteration_reason = "missing_tool_turn_replay".to_string();
                            continue;
                        }
                    }
                    let should_try_context_recovery = !context_overflow_recovery_attempted
                        && (is_context_length_exceeded_error(err.as_str())
                            || is_request_body_too_large_error(err.as_str()));
                    if should_try_context_recovery {
                        if let Some(refresh) = &options.iterative_context_refresh {
                            info!(
                                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                                conversation_turn_id =
                                    options.conversation_turn_id.as_deref().unwrap_or(""),
                                iteration,
                                error = err.as_str(),
                                "ai runtime attempting context overflow recovery"
                            );
                            context_overflow_recovery_attempted = true;
                            match refresh
                                .try_recover_from_context_overflow(&options.callbacks)
                                .await
                            {
                                Ok(true) => {
                                    iteration_reason = "context_overflow_recovery".to_string();
                                    continue;
                                }
                                Ok(false) => {}
                                Err(recovery_err) => {
                                    warn!(
                                        "memory active summary recovery failed: {}",
                                        recovery_err
                                    );
                                }
                            }
                        }
                    }
                    return Err(err);
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
                if response.content.trim().is_empty() {
                    if !empty_final_response_followup_attempted && iteration < self.max_iterations {
                        warn!(
                            conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                            conversation_turn_id =
                                options.conversation_turn_id.as_deref().unwrap_or(""),
                            iteration,
                            response_id = response.response_id.as_deref().unwrap_or(""),
                            finish_reason = response.finish_reason.as_deref().unwrap_or(""),
                            "ai runtime received empty final response; asking model for final result"
                        );
                        empty_final_response_followup_attempted = true;
                        runtime_followup_items = vec![empty_final_response_followup_item()];
                        runtime_followup_appended_to_request = false;
                        iteration_reason = "empty_final_response_followup".to_string();
                        continue;
                    }
                    warn!(
                        conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                        conversation_turn_id =
                            options.conversation_turn_id.as_deref().unwrap_or(""),
                        iteration,
                        response_id = response.response_id.as_deref().unwrap_or(""),
                        finish_reason = response.finish_reason.as_deref().unwrap_or(""),
                        "ai runtime failed after empty final response"
                    );
                    return Err(EMPTY_FINAL_RESPONSE_ERROR.to_string());
                }
                info!(
                    conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                    conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                    iteration,
                    response_id = response.response_id.as_deref().unwrap_or(""),
                    finish_reason = response.finish_reason.as_deref().unwrap_or(""),
                    content_chars = response.content.chars().count(),
                    "ai runtime completed without tool calls"
                );
                self.save_assistant_record(&options, &response, response.tool_calls.clone(), None)
                    .await?;
                return Ok(AiRuntimeResult {
                    content: response.content,
                    reasoning: response.reasoning,
                    tool_calls: response.tool_calls,
                    finish_reason: response.finish_reason,
                    usage: response.usage,
                    response_id: response.response_id,
                });
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
            )
            .await?;

            let Some(executor) = &self.tool_executor else {
                return Ok(AiRuntimeResult {
                    content: response.content,
                    reasoning: response.reasoning,
                    tool_calls: response.tool_calls,
                    finish_reason: response.finish_reason,
                    usage: response.usage,
                    response_id: response.response_id,
                });
            };

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
            let tool_results = executor
                .execute_tools_stream(
                    tool_calls.as_array().map(Vec::as_slice).unwrap_or(&[]),
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
            let tool_call_items =
                build_tool_call_items(tool_calls.as_array().map(Vec::as_slice).unwrap_or(&[]));
            let tool_output_items = build_tool_output_items_with_budget(
                tool_results.as_slice(),
                options.tool_result_model_budget_limits,
            );
            info!(
                conversation_id = options.conversation_id.as_deref().unwrap_or(""),
                conversation_turn_id = options.conversation_turn_id.as_deref().unwrap_or(""),
                iteration,
                tool_result_count,
                tool_result_names = tool_result_names.join(", "),
                "ai runtime finished tool execution"
            );
            self.save_tool_records(&options, tool_results.as_slice())
                .await?;
            pending_tool_calls = Some(tool_call_items);
            pending_tool_outputs = Some(tool_output_items);
            if options.iterative_context_refresh.is_none() {
                request.input = append_tool_results_with_budget(
                    request.input,
                    request.supports_responses,
                    &response.content,
                    &tool_calls,
                    tool_results,
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
                metadata: options.record_options.assistant_metadata.clone(),
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

fn should_persist_tool_result(result: &ToolResult) -> bool {
    if !result.success || result.is_error || result.is_stream {
        return true;
    }

    let structured_empty_array = matches!(
        result.result.as_ref(),
        Some(Value::Array(items)) if items.is_empty()
    );
    if !structured_empty_array {
        return true;
    }

    result.content.trim() != "[]"
}

fn notify_context_overflow_recovery(callbacks: &RuntimeCallbacks, message: &str) {
    if let Some(cb) = &callbacks.on_thinking {
        cb(message.to_string());
    }
}

fn normalized_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn input_item_count(input: &Value) -> usize {
    input
        .as_array()
        .map(Vec::len)
        .unwrap_or(usize::from(!input.is_null()))
}

fn json_value_size_bytes(value: &Value) -> usize {
    value.to_string().len()
}

fn attach_runtime_debug(mut payload: Value, runtime_debug: &Value) -> Value {
    if let Some(map) = payload.as_object_mut() {
        map.insert("task_runner_debug".to_string(), runtime_debug.clone());
        payload
    } else {
        json!({
            "payload": payload,
            "task_runner_debug": runtime_debug,
        })
    }
}

fn merge_pending_tool_turn_into_input(
    input: Value,
    pending_tool_calls: Option<&[Value]>,
    pending_tool_outputs: Option<&[Value]>,
) -> Value {
    if pending_tool_calls.is_none() && pending_tool_outputs.is_none() {
        return input;
    }

    let mut items = input.as_array().cloned().unwrap_or_else(|| {
        if input.is_null() {
            Vec::new()
        } else {
            vec![input]
        }
    });
    merge_pending_tool_turn_items(&mut items, pending_tool_calls, pending_tool_outputs);
    Value::Array(items)
}

fn append_runtime_input_items(input: Value, items: &[Value]) -> Value {
    if items.is_empty() {
        return input;
    }
    let mut input_items = runtime_input_value_to_items(input);
    input_items.extend(items.iter().cloned());
    Value::Array(input_items)
}

fn runtime_input_value_to_items(input: Value) -> Vec<Value> {
    match input {
        Value::Array(items) => items,
        Value::String(text) => vec![json!({"role": "user", "content": text})],
        Value::Null => Vec::new(),
        other => vec![json!({"role": "user", "content": other})],
    }
}

fn empty_final_response_followup_item() -> Value {
    json!({
        "role": "user",
        "content": EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT,
    })
}

fn summarize_tool_call_names(tool_calls: &Value, limit: usize) -> Vec<String> {
    tool_calls
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool_call| extract_tool_call_name(tool_call).map(ToOwned::to_owned))
        .take(limit)
        .collect()
}

fn summarize_tool_result_names(tool_results: &[ToolResult], limit: usize) -> Vec<String> {
    tool_results
        .iter()
        .map(|result| result.name.clone())
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value};

    use chatos_mcp_runtime::{ToolCallerModelRuntime, ToolResult};

    use super::{
        append_runtime_input_items, empty_final_response_followup_item,
        merge_pending_tool_turn_into_input, should_persist_tool_result, AiRuntimeOptions,
        AiRuntimeResult, AiTurnReport, AiTurnStatus, IterativeContextRefresh,
        EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT,
    };

    #[test]
    fn runtime_options_pass_abort_checker_to_tool_context() {
        let options =
            AiRuntimeOptions::new(Some("session_1".to_string()), Some("turn_1".to_string()))
                .with_caller_model(Some("model_1".to_string()))
                .with_caller_model_runtime(Some(
                    ToolCallerModelRuntime::openai_compatible(
                        "https://example.com/v1",
                        "secret",
                        "model_1",
                        "gpt",
                    )
                    .with_responses_support(true)
                    .with_images_support(Some(true)),
                ))
                .with_abort_checker(Some(Arc::new(|session_id| session_id == "session_1")));

        assert!(options.is_aborted());
        let context = options.tool_call_context();
        assert_eq!(context.conversation_id.as_deref(), Some("session_1"));
        assert_eq!(context.conversation_turn_id.as_deref(), Some("turn_1"));
        assert_eq!(context.caller_model.as_deref(), Some("model_1"));
        let caller_runtime = context
            .caller_model_runtime
            .as_ref()
            .expect("caller runtime");
        assert_eq!(caller_runtime.model, "model_1");
        assert_eq!(caller_runtime.base_url, "https://example.com/v1");
        assert!(caller_runtime.supports_responses);
        assert_eq!(caller_runtime.supports_images, Some(true));
        assert!(context.is_aborted());
    }

    #[test]
    fn turn_report_wraps_success_and_failure() {
        let report = AiRuntimeResult {
            content: "done".to_string(),
            reasoning: Some("because".to_string()),
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: Some("resp_1".to_string()),
        }
        .into_report();

        assert_eq!(report.status, AiTurnStatus::Completed);
        assert!(report.is_completed());
        assert_eq!(report.content.as_deref(), Some("done"));
        assert_eq!(report.response_id.as_deref(), Some("resp_1"));

        let failed = AiTurnReport::failed("provider failed");
        assert_eq!(failed.status, AiTurnStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("provider failed"));

        let aborted = AiTurnReport::failed("aborted");
        assert_eq!(aborted.status, AiTurnStatus::Aborted);
        assert!(aborted.is_aborted());
        assert_eq!(aborted.user_message(), "任务已取消。");
        assert!(failed.user_message().contains("任务执行失败"));
        assert!(report.user_message().contains("done"));
    }

    #[tokio::test]
    async fn iterative_context_refresh_composes_prefix_and_sticky_items() {
        let input = IterativeContextRefresh::new(
            None,
            None,
            vec![json!({"role":"system","content":"prefix"})],
        )
        .with_sticky_input_items(vec![json!({"role":"user","content":"current"})])
        .compose_input()
        .await
        .expect("iterative input");

        let items = input.as_array().expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["content"].as_str(), Some("prefix"));
        assert_eq!(items[1]["content"].as_str(), Some("current"));
    }

    #[test]
    fn merge_pending_tool_turn_into_input_repairs_refreshed_context() {
        let input = json!([
            {"type":"message","role":"user","content":[]},
            {"type":"function_call","call_id":"call_1","name":"search","arguments":"{}"}
        ]);
        let pending_calls = vec![
            json!({"type":"function_call","call_id":"call_1","name":"search","arguments":"{}"}),
        ];
        let pending_outputs =
            vec![json!({"type":"function_call_output","call_id":"call_1","output":"done"})];

        let merged = merge_pending_tool_turn_into_input(
            input,
            Some(pending_calls.as_slice()),
            Some(pending_outputs.as_slice()),
        );
        let items = merged.as_array().expect("items");

        assert_eq!(
            items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                })
                .count(),
            1
        );
        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
        }));
    }

    #[test]
    fn append_runtime_input_items_wraps_string_input_for_empty_final_followup() {
        let followup = empty_final_response_followup_item();
        let merged =
            append_runtime_input_items(Value::String("do the task".to_string()), &[followup]);
        let items = merged.as_array().expect("items");

        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["role"].as_str(), Some("user"));
        assert_eq!(items[0]["content"].as_str(), Some("do the task"));
        assert_eq!(items[1]["role"].as_str(), Some("user"));
        assert_eq!(
            items[1]["content"].as_str(),
            Some(EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT)
        );
    }

    #[test]
    fn append_runtime_input_items_preserves_existing_items_for_empty_final_followup() {
        let followup = empty_final_response_followup_item();
        let merged = append_runtime_input_items(
            json!([
                {"role":"system","content":"rules"},
                {"role":"user","content":"run"}
            ]),
            &[followup],
        );
        let items = merged.as_array().expect("items");

        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["role"].as_str(), Some("system"));
        assert_eq!(items[1]["role"].as_str(), Some("user"));
        assert_eq!(items[2]["role"].as_str(), Some("user"));
        assert_eq!(
            items[2]["content"].as_str(),
            Some(EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT)
        );
    }

    #[test]
    fn should_persist_tool_result_skips_successful_empty_arrays_only() {
        let empty_success = tool_result("[]", Some(json!([])), true, false, false);
        assert!(!should_persist_tool_result(&empty_success));

        let non_empty_success = tool_result("[1]", Some(json!([1])), true, false, false);
        assert!(should_persist_tool_result(&non_empty_success));

        let plain_text_brackets = tool_result("[]", None, true, false, false);
        assert!(should_persist_tool_result(&plain_text_brackets));

        let empty_error = tool_result("[]", Some(json!([])), false, true, false);
        assert!(should_persist_tool_result(&empty_error));

        let empty_stream = tool_result("[]", Some(json!([])), true, false, true);
        assert!(should_persist_tool_result(&empty_stream));
    }

    fn tool_result(
        content: &str,
        result: Option<Value>,
        success: bool,
        is_error: bool,
        is_stream: bool,
    ) -> ToolResult {
        ToolResult {
            tool_call_id: "call_1".to_string(),
            name: "task_runner_service_list_tasks".to_string(),
            success,
            is_error,
            is_stream,
            conversation_turn_id: Some("turn_1".to_string()),
            content: content.to_string(),
            result,
        }
    }
}
