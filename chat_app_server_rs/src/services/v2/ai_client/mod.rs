use serde_json::{json, Value};
use tracing::{info, warn};

use crate::config::Config;
use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::user_settings::AiClientSettings;
use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::conversation_summarizer::{ConversationSummarizer, SummaryOverrides};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::services::v2::message_manager::MessageManager;
use crate::utils::abort_registry;

mod history_tools;
mod token_compaction;

use self::history_tools::{
    drop_duplicate_tail, ensure_tool_responses, find_anchor_index, find_summary_index,
};
use self::token_compaction::{
    estimate_delta_stats, is_token_limit_error, token_limit_budget_from_error,
    truncate_messages_by_tokens,
};

#[derive(Clone)]
pub struct AiClientCallbacks {
    pub on_chunk: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    pub on_tools_start: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_stream: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_end: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_start: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_stream: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_end: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
}

pub struct AiClient {
    ai_request_handler: AiRequestHandler,
    mcp_tool_execute: McpToolExecute,
    message_manager: MessageManager,
    max_iterations: i64,
    summary_threshold: i64,
    summary_keep_last_n: i64,
    history_limit: i64,
    system_prompt: Option<String>,
    summary_system_prompt: Option<String>,
    dynamic_summary_enabled: bool,
    max_context_tokens: i64,
    target_summary_tokens: i64,
    merge_target_summary_tokens: i64,
    summary_bisect_enabled: bool,
    summary_bisect_max_depth: i64,
    summary_bisect_min_messages: i64,
    summary_retry_on_context_overflow: bool,
    summarizer: Option<ConversationSummarizer>,
    anchor_user_content: Option<Value>,
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
            summary_threshold: cfg.summary_message_limit,
            summary_keep_last_n: cfg.summary_keep_last_n,
            history_limit: 2,
            system_prompt: None,
            summary_system_prompt: None,
            dynamic_summary_enabled: cfg.dynamic_summary_enabled,
            max_context_tokens: cfg.summary_max_context_tokens,
            target_summary_tokens: cfg.summary_target_tokens,
            merge_target_summary_tokens: cfg.summary_merge_target_tokens,
            summary_bisect_enabled: cfg.summary_bisect_enabled,
            summary_bisect_max_depth: cfg.summary_bisect_max_depth,
            summary_bisect_min_messages: cfg.summary_bisect_min_messages,
            summary_retry_on_context_overflow: cfg.summary_retry_on_context_overflow,
            summarizer: None,
            anchor_user_content: None,
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
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
    ) -> Result<Value, String> {
        let mut all_messages: Vec<Value> = Vec::new();

        if let Some(prompt) = self.system_prompt.clone() {
            all_messages.push(json!({"role": "system", "content": prompt}));
        }
        if let Some(summary) = self.summary_system_prompt.clone() {
            all_messages.push(json!({"role": "system", "content": summary}));
        }

        let mut history_messages: Vec<Value> = Vec::new();
        if let Some(session_id) = session_id.clone() {
            if self.history_limit != 0 {
                let limit = if self.history_limit > 0 {
                    Some(self.history_limit)
                } else {
                    None
                };
                let mut mapped = Vec::new();
                let summary_limit = Some(2);
                let (summaries, history) = self
                    .message_manager
                    .get_session_history_with_summaries(&session_id, limit, summary_limit)
                    .await;
                let has_summary_table = !summaries.is_empty();
                if has_summary_table {
                    for summary in summaries {
                        if !summary.summary_text.is_empty() {
                            mapped.push(json!({"role": "system", "content": format!("以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}", summary.summary_text)}));
                        }
                    }
                }
                for msg in history {
                    if msg
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("type"))
                        .and_then(|v| v.as_str())
                        == Some("session_summary")
                    {
                        if has_summary_table {
                            continue;
                        }
                        if let Some(summary) = msg.summary.clone() {
                            mapped.push(json!({"role": "system", "content": format!("以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}", summary)}));
                        }
                        continue;
                    }
                    if msg.role == "tool" {
                        let mut content = msg.content;
                        if content.is_empty() && msg.metadata.is_some() {
                            content = msg
                                .metadata
                                .clone()
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                        }
                        mapped.push(json!({"role": "tool", "tool_call_id": msg.tool_call_id.clone().unwrap_or_default(), "content": content}));
                    } else {
                        let mut item = json!({"role": msg.role, "content": msg.content});
                        if let Some(tc) = msg.tool_calls {
                            item["tool_calls"] = tc;
                        }
                        if let Some(tc) = msg
                            .metadata
                            .clone()
                            .and_then(|m| m.get("toolCalls").cloned())
                        {
                            item["tool_calls"] = tc;
                        }
                        mapped.push(item);
                    }
                }
                history_messages = ensure_tool_responses(drop_duplicate_tail(mapped, &messages));
            }
        }

        all_messages.extend(history_messages);
        all_messages.extend(messages.clone());

        // anchor user content for dynamic summary
        self.anchor_user_content = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|v| v.as_str()) == Some("user"))
            .and_then(|m| m.get("content").cloned());

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
            purpose,
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
        iteration: i64,
    ) -> Result<Value, String> {
        let mut messages = messages;
        let mut iteration = iteration;
        let purpose = purpose.unwrap_or_else(|| "chat".to_string());
        let persist_tool_messages = purpose != "sub_agent_router";

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

            // dynamic summary (in-memory) if enabled
            let mut api_messages = messages.clone();
            if self.dynamic_summary_enabled {
                if self.summarizer.is_none() {
                    self.summarizer = Some(ConversationSummarizer::new(
                        self.ai_request_handler.clone(),
                        self.message_manager.clone(),
                    ));
                }
                if let Some(summarizer) = &self.summarizer {
                    let idx_last_summary =
                        find_summary_index(&api_messages, self.summary_system_prompt.as_ref());
                    let idx_anchor =
                        find_anchor_index(&api_messages, self.anchor_user_content.as_ref());
                    let start_idx = if idx_last_summary >= 0 {
                        idx_last_summary
                    } else {
                        idx_anchor
                    };
                    let delta = if start_idx >= 0 {
                        api_messages[(start_idx as usize + 1)..].to_vec()
                    } else {
                        Vec::new()
                    };

                    let (delta_tokens, delta_count) = estimate_delta_stats(&delta);
                    let trigger_by_count =
                        self.summary_threshold > 0 && delta_count >= self.summary_threshold;
                    let trigger_by_tokens =
                        self.max_context_tokens > 0 && delta_tokens >= self.max_context_tokens;

                    if trigger_by_count || trigger_by_tokens {
                        let mut input = Vec::new();
                        if let Some(summary_prompt) = self.summary_system_prompt.clone() {
                            input.push(json!({"role": "system", "content": summary_prompt}));
                        } else if let Some(anchor) = self.anchor_user_content.clone() {
                            input.push(json!({"role": "user", "content": anchor}));
                        }
                        input.extend(delta);

                        let summary_callbacks =
                            crate::services::v2::conversation_summarizer::SummaryCallbacks {
                                on_start: callbacks.on_context_summarized_start.clone(),
                                on_stream: callbacks.on_context_summarized_stream.clone().map(
                                    |cb| {
                                        std::sync::Arc::new(move |chunk: String| {
                                            cb(Value::String(chunk));
                                        })
                                            as std::sync::Arc<dyn Fn(String) + Send + Sync>
                                    },
                                ),
                                on_end: callbacks.on_context_summarized_end.clone(),
                            };

                        let res = summarizer
                            .maybe_summarize_in_memory(
                                &input,
                                Some(SummaryOverrides {
                                    message_limit: Some(1),
                                    max_context_tokens: Some(1),
                                    keep_last_n: Some(0),
                                    target_summary_tokens: Some(self.target_summary_tokens),
                                    merge_target_tokens: Some(self.merge_target_summary_tokens),
                                    model: Some(model.clone()),
                                    temperature: Some(0.2),
                                    bisect_enabled: Some(self.summary_bisect_enabled),
                                    bisect_max_depth: Some(self.summary_bisect_max_depth),
                                    bisect_min_messages: Some(self.summary_bisect_min_messages),
                                    retry_on_context_overflow: Some(
                                        self.summary_retry_on_context_overflow,
                                    ),
                                }),
                                session_id.clone(),
                                true,
                                Some(summary_callbacks),
                            )
                            .await;

                        match res {
                            Ok(res) => {
                                if res.summarized {
                                    self.summary_system_prompt = res.system_prompt.clone();
                                    let mut rebuilt = Vec::new();
                                    if let Some(prompt) = self.system_prompt.clone() {
                                        rebuilt.push(json!({"role": "system", "content": prompt}));
                                    }
                                    if let Some(anchor) = self.anchor_user_content.clone() {
                                        rebuilt.push(json!({"role": "user", "content": anchor}));
                                    }
                                    if let Some(summary_prompt) = self.summary_system_prompt.clone()
                                    {
                                        rebuilt.push(
                                            json!({"role": "system", "content": summary_prompt}),
                                        );
                                    }
                                    api_messages = rebuilt;
                                }
                            }
                            Err(err) => {
                                warn!("[SUM-MEM] failed: {}", err);
                            }
                        }
                    }
                }
            }

            let mut resp = None;
            let mut last_err: Option<String> = None;
            let mut token_limit_compacted = false;
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
                        callbacks.on_chunk.is_some() || callbacks.on_thinking.is_some(),
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
                            if let Some(compacted) = self
                                .try_compact_for_token_limit(
                                    &api_messages,
                                    &err,
                                    &callbacks,
                                    session_id.clone(),
                                    &model,
                                )
                                .await
                            {
                                api_messages = compacted;
                                continue;
                            }
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
                new_messages.push(json!({"role": "tool", "tool_call_id": result.tool_call_id, "content": result.content}));
            }

            messages = new_messages;
            iteration += 1;
        }
    }

    async fn try_compact_for_token_limit(
        &mut self,
        messages: &Vec<Value>,
        err: &str,
        callbacks: &AiClientCallbacks,
        session_id: Option<String>,
        model: &str,
    ) -> Option<Vec<Value>> {
        let summary_input_budget = if self.max_context_tokens > 0 {
            self.max_context_tokens
        } else {
            6000
        };

        if self.dynamic_summary_enabled {
            if self.summarizer.is_none() {
                self.summarizer = Some(ConversationSummarizer::new(
                    self.ai_request_handler.clone(),
                    self.message_manager.clone(),
                ));
            }
            if let Some(summarizer) = &self.summarizer {
                let (trimmed_for_summary, _) =
                    truncate_messages_by_tokens(messages, summary_input_budget);
                let summary_callbacks =
                    crate::services::v2::conversation_summarizer::SummaryCallbacks {
                        on_start: callbacks.on_context_summarized_start.clone(),
                        on_stream: callbacks.on_context_summarized_stream.clone().map(|cb| {
                            std::sync::Arc::new(move |chunk: String| {
                                cb(Value::String(chunk));
                            })
                                as std::sync::Arc<dyn Fn(String) + Send + Sync>
                        }),
                        on_end: callbacks.on_context_summarized_end.clone(),
                    };

                let res = summarizer
                    .retry_after_context_overflow_in_memory(
                        &trimmed_for_summary,
                        err,
                        Some(SummaryOverrides {
                            message_limit: Some(1),
                            max_context_tokens: Some(1),
                            keep_last_n: Some(0),
                            target_summary_tokens: Some(self.target_summary_tokens),
                            merge_target_tokens: Some(self.merge_target_summary_tokens),
                            model: Some(model.to_string()),
                            temperature: Some(0.2),
                            bisect_enabled: Some(self.summary_bisect_enabled),
                            bisect_max_depth: Some(self.summary_bisect_max_depth),
                            bisect_min_messages: Some(self.summary_bisect_min_messages),
                            retry_on_context_overflow: Some(self.summary_retry_on_context_overflow),
                        }),
                        session_id.clone(),
                        true,
                        Some(summary_callbacks),
                    )
                    .await;

                match res {
                    Ok(Some(res)) => {
                        if res.summarized {
                            self.summary_system_prompt = res.system_prompt.clone();
                            let mut rebuilt = Vec::new();
                            if let Some(prompt) = self.system_prompt.clone() {
                                rebuilt.push(json!({"role": "system", "content": prompt}));
                            }
                            if let Some(anchor) = self.anchor_user_content.clone() {
                                rebuilt.push(json!({"role": "user", "content": anchor}));
                            }
                            if let Some(summary_prompt) = self.summary_system_prompt.clone() {
                                rebuilt.push(json!({"role": "system", "content": summary_prompt}));
                            }
                            if !rebuilt.is_empty() {
                                return Some(rebuilt);
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        warn!("[SUM-MEM] retry summary failed: {}", err);
                    }
                }
            }
        }

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

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) {
            self.max_iterations = v;
        }
        if let Some(v) = effective
            .get("DYNAMIC_SUMMARY_ENABLED")
            .and_then(|v| v.as_bool())
        {
            self.dynamic_summary_enabled = v;
        }
        if let Some(v) = effective.get("HISTORY_LIMIT").and_then(|v| v.as_i64()) {
            self.history_limit = v.max(0);
        }
        if let Some(v) = effective
            .get("SUMMARY_MESSAGE_LIMIT")
            .and_then(|v| v.as_i64())
        {
            self.summary_threshold = v;
        }
        if let Some(v) = effective
            .get("SUMMARY_KEEP_LAST_N")
            .and_then(|v| v.as_i64())
        {
            self.summary_keep_last_n = v;
        }
        if let Some(v) = effective
            .get("SUMMARY_MAX_CONTEXT_TOKENS")
            .and_then(|v| v.as_i64())
        {
            self.max_context_tokens = v;
        }
        if let Some(v) = effective
            .get("SUMMARY_TARGET_TOKENS")
            .and_then(|v| v.as_i64())
        {
            self.target_summary_tokens = v;
        }
        if let Some(v) = effective
            .get("SUMMARY_MERGE_TARGET_TOKENS")
            .and_then(|v| v.as_i64())
        {
            self.merge_target_summary_tokens = v;
        }
        if let Some(v) = effective
            .get("SUMMARY_BISECT_ENABLED")
            .and_then(|v| v.as_bool())
        {
            self.summary_bisect_enabled = v;
        }
        if let Some(v) = effective
            .get("SUMMARY_BISECT_MAX_DEPTH")
            .and_then(|v| v.as_i64())
        {
            self.summary_bisect_max_depth = v.max(1);
        }
        if let Some(v) = effective
            .get("SUMMARY_BISECT_MIN_MESSAGES")
            .and_then(|v| v.as_i64())
        {
            self.summary_bisect_min_messages = v.max(1);
        }
        if let Some(v) = effective
            .get("SUMMARY_RETRY_ON_CONTEXT_OVERFLOW")
            .and_then(|v| v.as_bool())
        {
            self.summary_retry_on_context_overflow = v;
        }
    }
}
