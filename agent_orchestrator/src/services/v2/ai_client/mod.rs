use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use tracing::warn;

use crate::config::Config;
pub use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::runtime_guidance_manager::{
    runtime_guidance_manager, RuntimeGuidanceItem, DEFAULT_DRAIN_LIMIT,
};
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
    cap_tool_content_for_input, is_response_parse_error, is_transient_transport_or_parse_error,
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
}

impl AiClient {
    pub fn new(
        ai_request_handler: AiRequestHandler,
        mcp_tool_execute: McpToolExecute,
        message_manager: MessageManager,
    ) -> Self {
        let cfg = Config::get();
        Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            history_limit: 2,
            system_prompt: None,
            max_context_tokens: cfg.summary_max_context_tokens,
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
    }

    pub fn set_mcp_tool_execute(&mut self, mcp_tool_execute: McpToolExecute) {
        self.mcp_tool_execute = mcp_tool_execute;
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
                        callbacks.on_chunk.is_some() || callbacks.on_thinking.is_some(),
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
                        if is_transient_transport_or_parse_error(&err) {
                            let retry_kind = if is_response_parse_error(&err) {
                                "响应解析异常"
                            } else {
                                "网络波动"
                            };
                            if transient_retry_count < max_transient_retries {
                                transient_retry_count += 1;
                                let backoff_ms = 150_u64 * transient_retry_count as u64;
                                warn!(
                                    "[AI_V2] transient {} detected; retry {}/{} after {}ms: {}",
                                    retry_kind,
                                    transient_retry_count,
                                    max_transient_retries,
                                    backoff_ms,
                                    err
                                );
                                sleep(Duration::from_millis(backoff_ms)).await;
                                continue;
                            }
                            last_err = Some(format!(
                                "AI 请求失败：{}，已重试 {} 次，最后错误：{}",
                                retry_kind, max_transient_retries, err
                            ));
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

            let tool_calls = resp.tool_calls.clone();
            if tool_calls.is_none()
                || tool_calls
                    .as_ref()
                    .unwrap()
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true)
            {
                return Ok(json!({
                    "success": true,
                    "content": resp.content,
                    "reasoning": resp.reasoning,
                    "tool_calls": Value::Null,
                    "finish_reason": resp.finish_reason,
                    "iteration": iteration
                }));
            }

            let tool_calls_val = tool_calls.unwrap();
            if let Some(cb) = &callbacks.on_tools_start {
                cb(tool_calls_val.clone());
            }
            let tool_calls_arr = tool_calls_val.as_array().cloned().unwrap_or_default();

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    if persist_tool_messages {
                        let aborted_results = build_aborted_tool_results(&tool_calls_arr, None);
                        self.message_manager
                            .save_tool_results(sid, aborted_results.as_slice())
                            .await;
                    }
                    return Err("aborted".to_string());
                }
            }

            let on_tools_stream_cb =
                build_tool_stream_callback(callbacks.on_tools_stream.clone(), session_id.clone());

            let tool_results = self
                .mcp_tool_execute
                .execute_tools_stream(
                    &tool_calls_arr,
                    session_id.as_deref(),
                    turn_id.as_deref(),
                    Some(model.as_str()),
                    on_tools_stream_cb,
                )
                .await;

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    if persist_tool_messages {
                        let aborted_results = build_aborted_tool_results(
                            &tool_calls_arr,
                            Some(tool_results.as_slice()),
                        );
                        self.message_manager
                            .save_tool_results(sid, aborted_results.as_slice())
                            .await;
                    }
                    return Err("aborted".to_string());
                }
            }

            if let Some(cb) = &callbacks.on_tools_end {
                cb(json!({"tool_results": tool_results.clone()}));
            }

            if persist_tool_messages {
                if let Some(sid) = session_id.as_ref() {
                    self.message_manager
                        .save_tool_results(sid, tool_results.as_slice())
                        .await;
                }
            }

            let mut new_messages = api_messages.clone();
            let content_val = if resp.content.is_empty() {
                Value::Null
            } else {
                Value::String(resp.content.clone())
            };
            let mut assistant_msg = json!({"role": "assistant", "content": content_val});
            if reasoning_enabled {
                assistant_msg["reasoning_content"] =
                    Value::String(resp.reasoning.clone().unwrap_or_default());
            }
            if let Some(tc) = resp.tool_calls.clone() {
                assistant_msg["tool_calls"] = tc;
            }
            new_messages.push(assistant_msg);

            for result in &tool_results {
                new_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": result.tool_call_id,
                    "content": cap_tool_content_for_input(result.content.as_str())
                }));
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

    let drained =
        runtime_guidance_manager().drain_guidance(session_id, turn_id, DEFAULT_DRAIN_LIMIT);
    if drained.is_empty() {
        return Vec::new();
    }

    let mut messages = Vec::with_capacity(drained.len());
    for guidance_item in drained {
        messages.push(build_runtime_guidance_message(&guidance_item));
        if let Some(applied_item) =
            runtime_guidance_manager().mark_applied(session_id, turn_id, &guidance_item.guidance_id)
        {
            if let Some(cb) = &callbacks.on_runtime_guidance_applied {
                cb(json!({
                    "guidance_id": applied_item.guidance_id,
                    "turn_id": applied_item.turn_id,
                    "status": "applied",
                    "created_at": applied_item.created_at,
                    "applied_at": applied_item.applied_at,
                    "pending_count": runtime_guidance_manager().pending_count(session_id, turn_id),
                }));
            }
        }
    }

    messages
}

fn build_runtime_guidance_message(guidance_item: &RuntimeGuidanceItem) -> Value {
    json!({
        "role": "user",
        "content": format_runtime_guidance_instruction(guidance_item),
    })
}

fn format_runtime_guidance_instruction(guidance_item: &RuntimeGuidanceItem) -> String {
    guidance_item.content.trim().to_string()
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
