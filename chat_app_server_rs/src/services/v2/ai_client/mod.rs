use serde_json::{json, Value};
use tracing::info;

use crate::config::Config;
use crate::core::messages::{
    build_assistant_message_with_parts,
};
use crate::core::tool_call::build_tool_role_message;
pub use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::{
    build_ai_client_success_payload, completion_failed_error, execute_tool_lifecycle,
    handle_transient_retry,
};
use crate::services::runtime_guidance_manager::support::{
    build_runtime_guidance_applied_event, drain_runtime_guidance_items,
    format_runtime_guidance_instruction,
};
use crate::services::task_board_refresh_context::TaskBoardRefreshContextStore;
use crate::services::runtime_guidance_manager::RuntimeGuidanceItem;
use crate::services::user_settings::AiClientSettings;
use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::services::v2::message_manager::MessageManager;
use crate::utils::abort_registry;

mod context_memory;
mod history_tools;
mod runtime_support;
mod token_compaction;

use self::history_tools::{
    drop_duplicate_tail, ensure_tool_responses, sanitize_messages_for_request,
};
use self::runtime_support::{
    cap_tool_content_for_input,
};
use self::token_compaction::{
    is_token_limit_error, token_limit_budget_from_error, truncate_messages_by_tokens,
};

pub struct AiClient {
    ai_request_handler: AiRequestHandler,
    mcp_tool_execute: McpToolExecute,
    message_manager: MessageManager,
    max_iterations: i64,
    history_limit: i64,
    system_prompt: Option<String>,
    max_context_tokens: i64,
    task_board_refresh_context: TaskBoardRefreshContextStore,
}

impl AiClient {
    pub fn new(
        ai_request_handler: AiRequestHandler,
        mcp_tool_execute: McpToolExecute,
        message_manager: MessageManager,
    ) -> Result<Self, String> {
        let cfg = Config::try_get()?;
        Ok(Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            history_limit: 2,
            system_prompt: None,
            max_context_tokens: cfg.summary_max_context_tokens,
            task_board_refresh_context: TaskBoardRefreshContextStore::new(),
        })
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
    }

    pub fn set_mcp_tool_execute(&mut self, mcp_tool_execute: McpToolExecute) {
        self.mcp_tool_execute = mcp_tool_execute;
    }

    pub fn set_task_board_refresh_context(
        &mut self,
        session_id: Option<String>,
        turn_id: Option<String>,
        contact_system_prompt: Option<String>,
        builtin_mcp_system_prompt: Option<String>,
        command_system_prompt: Option<String>,
    ) {
        self.task_board_refresh_context.set(
            session_id,
            turn_id,
            contact_system_prompt,
            builtin_mcp_system_prompt,
            command_system_prompt,
        );
    }

    pub async fn process_request(
        &mut self,
        messages: Vec<Value>,
        session_id: Option<String>,
        turn_id: Option<String>,
        model: String,
        temperature: f64,
        max_tokens: Option<i64>,
        use_tools: bool,
        callbacks: AiClientCallbacks,
        reasoning_enabled: bool,
        provider: Option<String>,
        thinking_level: Option<String>,
        purpose: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        prefixed_messages: Vec<Value>,
    ) -> Result<Value, String> {
        let resolved_purpose = purpose.unwrap_or_else(|| "chat".to_string());
        let mut all_messages: Vec<Value> = Vec::new();

        if let Some(prompt) = self.system_prompt.clone() {
            all_messages.push(json!({"role": "system", "content": prompt}));
        }
        all_messages.extend(prefixed_messages);

        let mut history_messages: Vec<Value> = Vec::new();
        if session_id.is_some() {
            let mapped = self
                .load_memory_context_messages_for_scope(session_id.as_deref(), reasoning_enabled)
                .await;
            history_messages = ensure_tool_responses(drop_duplicate_tail(mapped, &messages));
        }

        all_messages.extend(history_messages);
        all_messages.extend(messages.clone());

        let tools = if use_tools {
            Some(self.mcp_tool_execute.get_available_tools())
        } else {
            None
        };

        self.process_with_tools(
            all_messages,
            tools,
            session_id,
            turn_id,
            model,
            temperature,
            max_tokens,
            callbacks,
            reasoning_enabled,
            provider,
            thinking_level,
            Some(resolved_purpose),
            message_mode,
            message_source,
            0,
        )
        .await
    }

    async fn process_with_tools(
        &mut self,
        messages: Vec<Value>,
        tools: Option<Vec<Value>>,
        session_id: Option<String>,
        turn_id: Option<String>,
        model: String,
        temperature: f64,
        max_tokens: Option<i64>,
        callbacks: AiClientCallbacks,
        reasoning_enabled: bool,
        provider: Option<String>,
        thinking_level: Option<String>,
        purpose: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        iteration: i64,
    ) -> Result<Value, String> {
        let mut messages = messages;
        let mut iteration = iteration;
        let purpose = purpose.unwrap_or_else(|| "chat".to_string());
        let persist_tool_messages = purpose != "agent_builder";
        loop {
            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    return Err("aborted".to_string());
                }
            }
            if iteration >= self.max_iterations {
                return Err("达到最大迭代次数".to_string());
            }

            info!(
                "AI request iteration {} messages {}",
                iteration,
                messages.len()
            );

            self.maybe_refresh_context_from_memory(
                purpose.as_str(),
                iteration,
                session_id.as_deref(),
                reasoning_enabled,
                &mut messages,
            )
            .await;

            let mut api_messages = messages.clone();
            let runtime_guidance_messages = drain_runtime_guidance_messages(
                session_id.as_deref(),
                turn_id.as_deref(),
                &callbacks,
            );
            if !runtime_guidance_messages.is_empty() {
                api_messages.extend(runtime_guidance_messages);
            }
            api_messages = sanitize_messages_for_request(api_messages);

            let mut resp = None;
            let mut last_err: Option<String> = None;
            let mut token_limit_compacted = false;
            let max_transient_retries = 5usize;
            let mut transient_retry_count = 0usize;
            loop {
                let attempt = self
                    .ai_request_handler
                    .handle_request(
                        api_messages.clone(),
                        tools.clone(),
                        model.clone(),
                        Some(temperature),
                        max_tokens,
                        StreamCallbacks {
                            on_chunk: callbacks.on_chunk.clone(),
                            on_thinking: callbacks.on_thinking.clone(),
                        },
                        reasoning_enabled,
                        provider.clone(),
                        thinking_level.clone(),
                        session_id.clone(),
                        turn_id.clone(),
                        message_mode.clone(),
                        message_source.clone(),
                        purpose.as_str(),
                    )
                    .await;

                match attempt {
                    Ok(r) => {
                        resp = Some(r);
                        break;
                    }
                    Err(err) => {
                        last_err = Some(err.clone());
                        if !token_limit_compacted && is_token_limit_error(&err) {
                            token_limit_compacted = true;
                            if let Some(compacted) =
                                self.try_compact_for_token_limit(&api_messages, &err).await
                            {
                                api_messages = compacted;
                                continue;
                            }
                        }
                        match handle_transient_retry(
                            "[AI_V2]",
                            &err,
                            &mut transient_retry_count,
                            max_transient_retries,
                        )
                        .await
                        {
                            Ok(true) => continue,
                            Err(error_message) => {
                                last_err = Some(error_message);
                            }
                            Ok(false) => {}
                        }
                        break;
                    }
                }
            }

            let resp = match resp {
                Some(r) => r,
                None => return Err(last_err.unwrap_or_else(|| "request failed".to_string())),
            };

            if let Some(err) = completion_failed_error(
                resp.finish_reason.as_deref(),
                resp.content.as_str(),
                resp.reasoning.as_deref(),
                None,
            ) {
                return Err(err);
            }

            let Some(tool_calls_val) = resp.tool_calls.clone().filter(|tool_calls| {
                tool_calls
                    .as_array()
                    .map(|items| !items.is_empty())
                    .unwrap_or(false)
            }) else {
                return Ok(build_ai_client_success_payload(
                    resp.content,
                    resp.reasoning,
                    resp.finish_reason,
                    iteration,
                ));
            };

            let tool_calls_arr = tool_calls_val.as_array().cloned().unwrap_or_default();
            let mcp_tool_execute = self.mcp_tool_execute.clone();
            let message_manager = self.message_manager.clone();
            let persist_session_id = session_id.clone();
            let tool_results = execute_tool_lifecycle(
                tool_calls_arr.as_slice(),
                tool_calls_val.clone(),
                session_id.as_deref(),
                persist_tool_messages,
                &callbacks,
                |on_tools_stream_cb| {
                    mcp_tool_execute.execute_tools_stream(
                        &tool_calls_arr,
                        session_id.as_deref(),
                        turn_id.as_deref(),
                        Some(model.as_str()),
                        on_tools_stream_cb,
                    )
                },
                |results| results.to_vec(),
                move |results| {
                    let message_manager = message_manager.clone();
                    let persist_session_id = persist_session_id.clone();
                    async move {
                        if let Some(sid) = persist_session_id.as_ref() {
                            message_manager
                            .save_tool_results(sid, results.as_slice())
                            .await;
                        }
                    }
                },
            )
            .await?
            .tool_results;

            let mut new_messages = api_messages.clone();
            let assistant_msg = build_assistant_message_with_parts(
                if resp.content.is_empty() {
                    Value::Null
                } else {
                    Value::String(resp.content.clone())
                },
                if reasoning_enabled {
                    resp.reasoning.as_deref()
                } else {
                    None
                },
                reasoning_enabled,
                resp.tool_calls.clone(),
            );
            new_messages.push(assistant_msg);

            for result in &tool_results {
                new_messages.push(build_tool_role_message(
                    result.tool_call_id.as_str(),
                    cap_tool_content_for_input(result.content.as_str()).as_str(),
                ));
            }

            messages = new_messages;
            iteration += 1;
        }
    }

    async fn try_compact_for_token_limit(
        &self,
        messages: &Vec<Value>,
        err: &str,
    ) -> Option<Vec<Value>> {
        let summary_input_budget = if self.max_context_tokens > 0 {
            self.max_context_tokens
        } else {
            6000
        };

        let budget = token_limit_budget_from_error(err)
            .unwrap_or(summary_input_budget)
            .max(1000);
        let (mut truncated, changed) = truncate_messages_by_tokens(messages, budget);
        if changed {
            truncated = ensure_tool_responses(truncated);
            return Some(truncated);
        }
        None
    }
}

fn drain_runtime_guidance_messages(
    session_id: Option<&str>,
    turn_id: Option<&str>,
    callbacks: &AiClientCallbacks,
) -> Vec<Value> {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let Some(turn_id) = turn_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };

    let drained = drain_runtime_guidance_items(Some(session_id), Some(turn_id));
    if drained.is_empty() {
        return Vec::new();
    }

    let mut messages = Vec::with_capacity(drained.len());
    for drained_item in drained {
        messages.push(build_runtime_guidance_message(&drained_item.guidance_item));
        if let Some(applied_item) = drained_item.applied_item {
            if let Some(cb) = &callbacks.on_runtime_guidance_applied {
                cb(build_runtime_guidance_applied_event(
                    &applied_item,
                    drained_item.pending_count,
                    false,
                ));
            }
        }
    }

    messages
}

fn build_runtime_guidance_message(guidance_item: &RuntimeGuidanceItem) -> Value {
    json!({
        "role": "system",
        "content": format_runtime_guidance_instruction(guidance_item),
    })
}

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) {
            self.max_iterations = v;
        }
        if let Some(v) = effective.get("HISTORY_LIMIT").and_then(|v| v.as_i64()) {
            self.history_limit = v.max(0);
        }
        if let Some(v) = effective
            .get("SUMMARY_MAX_CONTEXT_TOKENS")
            .and_then(|v| v.as_i64())
        {
            self.max_context_tokens = v;
        }
    }
}
