use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use tracing::warn;

use crate::config::Config;
use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::user_settings::AiClientSettings;
use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::services::v2::message_manager::MessageManager;
use crate::utils::abort_registry;

mod history_tools;
mod token_compaction;

use self::history_tools::{drop_duplicate_tail, ensure_tool_responses};
use self::token_compaction::{
    is_token_limit_error, token_limit_budget_from_error, truncate_messages_by_tokens,
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

    async fn load_memory_context_messages_for_scope(
        &self,
        session_id: Option<&str>,
        include_reasoning: bool,
    ) -> Vec<Value> {
        let mut mapped = Vec::new();
        let (merged_summary, _summary_count, history) = if let Some(sid) = session_id {
            self.message_manager
                .get_memory_chat_history_context(sid, 2)
                .await
        } else {
            (None, 0, Vec::new())
        };
        if let Some(summary_text) = merged_summary {
            mapped.push(json!({"role": "system", "content": summary_text}));
        }

        for msg in history {
            if msg
                .metadata
                .as_ref()
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
                == Some("session_summary")
            {
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
                content = cap_tool_content_for_input(content.as_str());
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
                if include_reasoning && msg.role == "assistant" {
                    let has_tool_calls = item
                        .get("tool_calls")
                        .map(|value| !value.is_null())
                        .unwrap_or(false);
                    if has_tool_calls {
                        item["reasoning_content"] =
                            Value::String(msg.reasoning.clone().unwrap_or_default());
                    } else if let Some(reasoning) = msg.reasoning.clone() {
                        if !reasoning.trim().is_empty() {
                            item["reasoning_content"] = Value::String(reasoning);
                        }
                    }
                }
                mapped.push(item);
            }
        }

        mapped
    }

    async fn maybe_refresh_context_from_memory(
        &self,
        purpose: &str,
        iteration: i64,
        session_id: Option<&str>,
        include_reasoning: bool,
        messages: &mut Vec<Value>,
    ) {
        if purpose != "chat" || iteration <= 0 {
            return;
        }
        if session_id.is_none() {
            return;
        }

        let mut refreshed = Vec::new();
        if let Some(prompt) = self.system_prompt.clone() {
            refreshed.push(json!({"role": "system", "content": prompt}));
        }
        let mapped = self
            .load_memory_context_messages_for_scope(session_id, include_reasoning)
            .await;
        refreshed.extend(ensure_tool_responses(mapped));
        if refreshed != *messages {
            info!(
                "[AI_V2] context refreshed from memory_context: old_messages={}, new_messages={}",
                messages.len(),
                refreshed.len()
            );
            *messages = refreshed;
        }
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
    ) -> Result<Value, String> {
        let resolved_purpose = purpose.unwrap_or_else(|| "chat".to_string());
        let mut all_messages: Vec<Value> = Vec::new();

        if let Some(prompt) = self.system_prompt.clone() {
            all_messages.push(json!({"role": "system", "content": prompt}));
        }

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

fn is_response_parse_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("invalid json response")
        || message.contains("stream response parse failed")
        || message.contains("error decoding response body")
        || message.contains("unexpected end of json input")
        || message.contains("eof while parsing")
}

fn is_transient_network_error(err: &str) -> bool {
    let message = err.to_lowercase();
    message.contains("error sending request for url")
        || message.contains("connection closed before message completed")
        || message.contains("connection reset")
        || message.contains("broken pipe")
        || message.contains("connection refused")
        || message.contains("network is unreachable")
        || message.contains("unexpected eof")
        || message.contains("timed out")
        || message.contains("timeout")
        || message.contains("dns error")
        || message.contains("temporary failure in name resolution")
        || message.contains("failed to lookup address information")
        || message.contains("status 408")
        || message.contains("status 502")
        || message.contains("status 503")
        || message.contains("status 504")
        || message.contains("status 522")
        || message.contains("status 523")
        || message.contains("status 524")
        || message.contains("engine_overloaded_error")
        || message.contains("currently overloaded, please try again later")
        || message.contains("server is currently overloaded")
}

fn is_transient_transport_or_parse_error(err: &str) -> bool {
    is_transient_network_error(err) || is_response_parse_error(err)
}

fn tool_content_item_max_chars() -> usize {
    std::env::var("AI_V2_TOOL_OUTPUT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(8_000)
}

fn cap_tool_content_for_input(raw: &str) -> String {
    truncate_text_keep_tail(raw, tool_content_item_max_chars())
}

fn truncate_text_keep_tail(raw: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total = raw.chars().count();
    if total <= max_chars {
        return raw.to_string();
    }

    let marker = format!("[...truncated {} chars...]\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return raw
            .chars()
            .rev()
            .take(max_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }

    let keep_tail = max_chars - marker_chars;
    let tail: String = raw
        .chars()
        .rev()
        .take(keep_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}", marker, tail)
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

#[cfg(test)]
mod tests {
    use super::{
        cap_tool_content_for_input, is_response_parse_error, is_transient_network_error,
        is_transient_transport_or_parse_error,
    };

    #[test]
    fn cap_tool_content_for_input_truncates_large_text() {
        let text = "a".repeat(20_000);
        let truncated = cap_tool_content_for_input(text.as_str());
        assert!(truncated.len() < text.len());
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn cap_tool_content_for_input_keeps_short_text() {
        let text = "short output";
        assert_eq!(cap_tool_content_for_input(text), text.to_string());
    }

    #[test]
    fn detects_response_parse_errors() {
        assert!(is_response_parse_error(
            "invalid JSON response (status 200): expected value"
        ));
        assert!(is_response_parse_error(
            "stream response parse failed: no valid SSE events parsed from provider"
        ));
        assert!(!is_response_parse_error("status 401: unauthorized"));
    }

    #[test]
    fn detects_transient_network_errors() {
        assert!(is_transient_network_error(
            "error sending request for url (https://api.openai.com/v1/chat/completions)"
        ));
        assert!(is_transient_network_error(
            "status 503: service unavailable"
        ));
        assert!(is_transient_network_error(
            "{\"error\":{\"message\":\"The engine is currently overloaded, please try again later\",\"type\":\"engine_overloaded_error\"}}"
        ));
        assert!(!is_transient_network_error("status 401: invalid api key"));
    }

    #[test]
    fn combines_transient_network_and_parse_detection() {
        assert!(is_transient_transport_or_parse_error(
            "error decoding response body: unexpected eof"
        ));
        assert!(is_transient_transport_or_parse_error(
            "status 504: gateway timeout"
        ));
        assert!(!is_transient_transport_or_parse_error(
            "status 400: invalid_request_error"
        ));
    }
}
