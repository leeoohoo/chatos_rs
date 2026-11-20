// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use chatos_mcp_runtime::ToolResult;

use crate::request::AiRequestHandler;
use crate::tool_call::tool_calls_value_has_items;
use crate::traits::SaveToolRecordInput;
use crate::traits::{
    MemoryRecordWriter, ModelRequest, SaveAssistantRecordInput, SaveRecordInput, ToolExecutor,
};
use crate::{RuntimeBeforeModelRequest, RuntimeIterationContext};

mod final_response;
mod input_items;
mod model_request;
mod options;
mod persistence;
mod report;
mod single_step;

pub use self::options::AiRuntimeOptions;
pub use self::report::AiRuntimeResult;
pub use self::single_step::{AiSingleStepOutcome, AiSingleStepRequest};

use self::final_response::runtime_result_from_response;
use self::input_items::append_lifecycle_input_items;
use self::persistence::normalized_option;
use self::persistence::should_persist_tool_result;

pub struct AiRuntime {
    request_handler: AiRequestHandler,
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    record_writer: Option<Arc<dyn MemoryRecordWriter>>,
    max_iterations: usize,
}

const EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT: &str = "上一轮响应没有返回任何可展示的最终结果。请先检查当前任务是否已经真实完成：如果已经满足目标且不需要更多验证，直接输出最终结果；如果仍有未完成工作、未处理的任务状态/门禁反馈、缺少关键事实或缺少验证，请继续使用必要工具完成工作或记录明确阻塞。不要把未完成工作包装成最终结果。";
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

    pub fn with_request_read_timeout(mut self, read_timeout: Duration) -> Self {
        self.request_handler = AiRequestHandler::new_with_read_timeout(read_timeout);
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
        let metadata = merge_record_metadata(
            merge_record_metadata(
                options.record_options.assistant_metadata.clone(),
                metadata_override,
            ),
            response.usage.as_ref().map(bounded_provider_usage_metadata),
        );
        let metadata = merge_record_metadata(
            metadata,
            (!response.response_output_items.is_empty()).then(|| {
                serde_json::json!({
                    "responses_output": Value::Array(response.response_output_items.clone()),
                })
            }),
        );
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
                metadata,
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

fn bounded_provider_usage_metadata(usage: &Value) -> Value {
    let snapshot = crate::compat::extract_usage_snapshot(usage);
    serde_json::json!({
        "provider_usage": {
            "input_tokens": snapshot.input_tokens,
            "cached_tokens": snapshot.cached_tokens,
            "output_tokens": snapshot.output_tokens,
        }
    })
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
        iteration_request.input = append_lifecycle_input_items(
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
