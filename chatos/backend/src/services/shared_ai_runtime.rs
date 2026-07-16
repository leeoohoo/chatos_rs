// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_agent::DEFAULT_AGENT_MAX_ITERATIONS;
use serde_json::Value;
use tracing::warn;

use crate::config::Config;
use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::messages::set_task_runner_async_overall_status_for_session;
use crate::services::access_token_scope;
use crate::services::agent_runtime::mcp_tool_execute::McpToolExecute;
use crate::services::agent_runtime::message_manager::MessageManager;
use crate::services::ai_common::TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE;
use crate::services::chatos_memory_engine::CHATOS_COMPAT_SOURCE_ID;

pub fn build_shared_ai_runtime(
    tool_executor: Option<McpToolExecute>,
) -> chatos_ai_runtime::AiRuntime {
    let tool_executor = tool_executor
        .map(ChatosToolExecutorAdapter::new)
        .map(|adapter| Arc::new(adapter) as Arc<dyn chatos_ai_runtime::ToolExecutor>);
    chatos_ai_runtime::AiRuntime::new(tool_executor)
}

pub fn build_shared_ai_runtime_with_chatos_records(
    tool_executor: Option<McpToolExecute>,
    message_manager: MessageManager,
) -> chatos_ai_runtime::AiRuntime {
    let writer = Arc::new(ChatosMemoryRecordWriterAdapter::new(message_manager))
        as Arc<dyn chatos_ai_runtime::MemoryRecordWriter>;
    build_shared_ai_runtime(tool_executor).with_record_writer(Some(writer))
}

pub fn build_shared_contextual_turn_runner(
    tool_executor: Option<McpToolExecute>,
    message_manager: MessageManager,
) -> Result<chatos_ai_runtime::ContextualTurnRunner, String> {
    build_shared_contextual_turn_runner_with_max_iterations(
        tool_executor,
        message_manager,
        DEFAULT_AGENT_MAX_ITERATIONS,
    )
}

pub fn build_shared_contextual_turn_runner_with_max_iterations(
    tool_executor: Option<McpToolExecute>,
    message_manager: MessageManager,
    max_iterations: usize,
) -> Result<chatos_ai_runtime::ContextualTurnRunner, String> {
    let cfg = Config::try_get()?;
    let runtime = build_shared_ai_runtime_with_chatos_records(tool_executor, message_manager)
        .with_max_iterations(max_iterations);
    let mut memory_client = memory_engine_sdk::MemoryEngineClient::new_direct(
        cfg.memory_engine_base_url.clone(),
        Duration::from_millis(cfg.memory_engine_request_timeout_ms.max(300) as u64),
        CHATOS_COMPAT_SOURCE_ID.to_string(),
    )?;
    if let Some(access_token) = access_token_scope::get_current_access_token() {
        memory_client = memory_client.with_bearer_token(access_token);
    } else if let Some(operator_token) = cfg.memory_engine_operator_token.as_deref() {
        memory_client = memory_client.with_internal_service_auth("chatos-backend", operator_token);
    }
    let composer = chatos_ai_runtime::MemoryContextComposer::from_client(memory_client);
    Ok(chatos_ai_runtime::ContextualTurnRunner::new(
        runtime,
        Some(composer),
    ))
}

#[derive(Clone)]
pub(crate) struct ChatosToolExecutorAdapter {
    executor: McpToolExecute,
}

impl ChatosToolExecutorAdapter {
    pub(crate) fn new(executor: McpToolExecute) -> Self {
        Self { executor }
    }
}

#[derive(Clone)]
pub(crate) struct ChatosMemoryRecordWriterAdapter {
    message_manager: MessageManager,
}

impl ChatosMemoryRecordWriterAdapter {
    pub(crate) fn new(message_manager: MessageManager) -> Self {
        Self { message_manager }
    }
}

#[async_trait]
impl chatos_ai_runtime::MemoryRecordWriter for ChatosMemoryRecordWriterAdapter {
    async fn save_record(&self, input: chatos_ai_runtime::SaveRecordInput) -> Result<(), String> {
        match input.role.as_str() {
            "user" => {
                let metadata = input.packed_metadata();
                let task_runner_async_plan = input.message_mode.as_deref().map(str::trim)
                    == Some(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE);
                let message_id = input.message_id.clone();
                self.message_manager
                    .save_user_message(
                        input.conversation_id.as_str(),
                        input.content.as_str(),
                        input.message_id,
                        input.message_mode,
                        input.message_source,
                        metadata,
                    )
                    .await?;
                if task_runner_async_plan {
                    if let Some(message_id) = message_id.as_deref() {
                        if let Err(err) = set_task_runner_async_overall_status_for_session(
                            input.conversation_id.as_str(),
                            message_id,
                            "processing",
                        )
                        .await
                        {
                            warn!(
                                conversation_id = input.conversation_id.as_str(),
                                user_message_id = message_id,
                                error = err.as_str(),
                                "task runner async processing status persist failed"
                            );
                        }
                    }
                }
                Ok(())
            }
            "assistant" => {
                let metadata = input.packed_metadata();
                let response_id = input.response_id.clone();
                let turn_id = input.conversation_turn_id.clone();
                let response_status = input.response_status.clone();
                self.message_manager
                    .save_assistant_response_message(
                        input.conversation_id.as_str(),
                        input.content.as_str(),
                        input.reasoning,
                        input.message_mode,
                        input.message_source,
                        metadata,
                        input.tool_calls,
                        response_id.as_deref(),
                        turn_id.as_deref(),
                        response_status.as_deref(),
                    )
                    .await
                    .map(|_| ())
            }
            "tool" => {
                let tool_call_id = input.tool_call_id.clone().unwrap_or_default();
                let metadata = input.packed_metadata();
                self.message_manager
                    .save_tool_message(
                        input.conversation_id.as_str(),
                        input.content.as_str(),
                        tool_call_id.as_str(),
                        input.message_mode,
                        input.message_source,
                        metadata,
                    )
                    .await
                    .map(|_| ())
            }
            other => Err(format!("unsupported runtime record role: {other}")),
        }
    }

    async fn save_assistant_record(
        &self,
        input: chatos_ai_runtime::SaveAssistantRecordInput,
    ) -> Result<(), String> {
        let response_id = input.response_id.clone();
        let turn_id = input.conversation_turn_id.clone();
        let response_status = input.response_status.clone();
        self.message_manager
            .save_assistant_response_message(
                input.conversation_id.as_str(),
                input.content.as_str(),
                input.reasoning,
                input.message_mode,
                input.message_source,
                input.metadata,
                input.tool_calls,
                response_id.as_deref(),
                turn_id.as_deref(),
                response_status.as_deref(),
            )
            .await
            .map(|_| ())
    }

    async fn save_tool_record(
        &self,
        input: chatos_ai_runtime::SaveToolRecordInput,
    ) -> Result<(), String> {
        let record: chatos_ai_runtime::SaveRecordInput = input.clone().into();
        self.message_manager
            .save_tool_message(
                input.conversation_id.as_str(),
                input.content.as_str(),
                input.tool_call_id.as_str(),
                input.message_mode,
                input.message_source,
                record.packed_metadata(),
            )
            .await
            .map(|_| ())
    }
}

#[async_trait]
impl chatos_ai_runtime::ToolExecutor for ChatosToolExecutorAdapter {
    fn available_tools(&self) -> Vec<Value> {
        self.executor.get_available_tools()
    }

    async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: chatos_mcp_runtime::ToolCallContext,
        on_tool_result: Option<chatos_mcp_runtime::ToolResultCallback>,
    ) -> Vec<chatos_mcp_runtime::ToolResult> {
        self.executor
            .execute_tools_stream(
                tool_calls,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                context.caller_model.as_deref(),
                context.caller_model_runtime.as_ref(),
                on_tool_result,
            )
            .await
    }
}

pub fn shared_model_request(
    base_url: String,
    api_key: String,
    model: String,
    provider: String,
    input: Value,
    tools: Vec<Value>,
    supports_responses: bool,
    instructions: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    thinking_level: Option<String>,
) -> chatos_ai_runtime::ModelRequest {
    shared_model_request_with_options(
        base_url,
        api_key,
        model,
        provider,
        input,
        tools,
        supports_responses,
        instructions,
        temperature,
        max_output_tokens,
        thinking_level,
        None,
        None,
        false,
        None,
    )
}

pub fn shared_model_runtime_config_from_resolved(
    resolved: &ResolvedChatModelConfig,
) -> chatos_ai_runtime::ModelRuntimeConfig {
    chatos_ai_runtime::ModelRuntimeConfig::openai_compatible(
        resolved.base_url.clone(),
        resolved.api_key.clone(),
        resolved.model.clone(),
        resolved.provider.clone(),
    )
    .with_responses_support(resolved.supports_responses)
    .with_images_support(Some(resolved.supports_images))
    .with_temperature(Some(resolved.temperature))
    .with_thinking_level(resolved.thinking_level.clone())
    .with_instructions(resolved.system_prompt.clone())
}

pub async fn resolve_shared_model_runtime_config_for_request(
    requested_model_config_id: Option<&str>,
    request_model_cfg: Option<&Value>,
    session_id: Option<&str>,
    user_id: Option<&str>,
    default_model: &str,
    request_reasoning_enabled: Option<bool>,
    respect_model_flags: bool,
) -> Result<chatos_ai_runtime::ModelRuntimeConfig, String> {
    let resolved = crate::services::model_runtime_resolver::resolve_model_runtime_for_request(
        requested_model_config_id,
        request_model_cfg,
        session_id,
        user_id,
        default_model,
        request_reasoning_enabled,
        respect_model_flags,
    )
    .await?;
    Ok(shared_model_runtime_config_from_resolved(&resolved))
}

#[allow(clippy::too_many_arguments)]
pub fn shared_model_request_with_options(
    base_url: String,
    api_key: String,
    model: String,
    provider: String,
    input: Value,
    tools: Vec<Value>,
    supports_responses: bool,
    instructions: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    thinking_level: Option<String>,
    prompt_cache_key: Option<String>,
    request_cwd: Option<String>,
    include_prompt_cache_retention: bool,
    request_body_limit_bytes: Option<usize>,
) -> chatos_ai_runtime::ModelRequest {
    chatos_ai_runtime::ModelRequest::openai_compatible(base_url, api_key, model, provider, input)
        .with_responses_support(supports_responses)
        .with_instructions(instructions)
        .with_tools(tools)
        .with_temperature(temperature)
        .with_max_output_tokens(max_output_tokens)
        .with_thinking_level(thinking_level)
        .with_prompt_cache_key(prompt_cache_key)
        .with_request_cwd(request_cwd)
        .with_prompt_cache_retention(include_prompt_cache_retention)
        .with_request_body_limit_bytes(request_body_limit_bytes)
}
