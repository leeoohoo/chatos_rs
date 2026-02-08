use serde_json::{Value, json};
use tracing::{info, warn};
use std::collections::HashSet;

use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::message_manager::MessageManager;
use crate::services::v2::mcp_tool_execute::{McpToolExecute, ToolResult};
use crate::services::v2::conversation_summarizer::{ConversationSummarizer, SummaryOverrides};
use crate::utils::abort_registry;
use crate::services::user_settings::AiClientSettings;
use crate::config::Config;

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
    summarizer: Option<ConversationSummarizer>,
    anchor_user_content: Option<Value>,
}

impl AiClient {
    pub fn new(ai_request_handler: AiRequestHandler, mcp_tool_execute: McpToolExecute, message_manager: MessageManager) -> Self {
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
        model: String,
        temperature: f64,
        max_tokens: Option<i64>,
        use_tools: bool,
        callbacks: AiClientCallbacks,
        reasoning_enabled: bool,
        provider: Option<String>,
        thinking_level: Option<String>,
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
                let limit = if self.history_limit > 0 { Some(self.history_limit) } else { None };
                let mut mapped = Vec::new();
                let summary_limit = Some(2);
                let (summaries, history) = self.message_manager.get_session_history_with_summaries(&session_id, limit, summary_limit).await;
                let has_summary_table = !summaries.is_empty();
                if has_summary_table {
                    for summary in summaries {
                        if !summary.summary_text.is_empty() {
                            mapped.push(json!({"role": "system", "content": format!("以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}", summary.summary_text)}));
                        }
                    }
                }
                for msg in history {
                    if msg.metadata.as_ref().and_then(|m| m.get("type")).and_then(|v| v.as_str()) == Some("session_summary") {
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
                            content = msg.metadata.clone().map(|v| v.to_string()).unwrap_or_default();
                        }
                        mapped.push(json!({"role": "tool", "tool_call_id": msg.tool_call_id.clone().unwrap_or_default(), "content": content}));
                    } else {
                        let mut item = json!({"role": msg.role, "content": msg.content});
                        if let Some(tc) = msg.tool_calls { item["tool_calls"] = tc; }
                        if let Some(tc) = msg.metadata.clone().and_then(|m| m.get("toolCalls").cloned()) {
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
        self.anchor_user_content = messages.iter().rev().find(|m| m.get("role").and_then(|v| v.as_str()) == Some("user"))
            .and_then(|m| m.get("content").cloned());

        let tools = if use_tools { Some(self.mcp_tool_execute.get_available_tools()) } else { None };

        self.process_with_tools(
            all_messages,
            tools,
            session_id,
            model,
            temperature,
            max_tokens,
            callbacks,
            reasoning_enabled,
            provider,
            thinking_level,
            0
        ).await
    }

    async fn process_with_tools(
        &mut self,
        messages: Vec<Value>,
        tools: Option<Vec<Value>>,
        session_id: Option<String>,
        model: String,
        temperature: f64,
        max_tokens: Option<i64>,
        callbacks: AiClientCallbacks,
        reasoning_enabled: bool,
        provider: Option<String>,
        thinking_level: Option<String>,
        iteration: i64,
    ) -> Result<Value, String> {
        let mut messages = messages;
        let mut iteration = iteration;

        loop {
            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) { return Err("aborted".to_string()); }
            }
            if iteration >= self.max_iterations {
                return Err("达到最大迭代次数".to_string());
            }

            info!("AI request iteration {} messages {}", iteration, messages.len());

            // dynamic summary (in-memory) if enabled
            let mut api_messages = messages.clone();
            if self.dynamic_summary_enabled {
                if self.summarizer.is_none() {
                    self.summarizer = Some(ConversationSummarizer::new(self.ai_request_handler.clone(), self.message_manager.clone()));
                }
                if let Some(summarizer) = &self.summarizer {
                    let idx_last_summary = find_summary_index(&api_messages, self.summary_system_prompt.as_ref());
                    let idx_anchor = find_anchor_index(&api_messages, self.anchor_user_content.as_ref());
                    let start_idx = if idx_last_summary >= 0 { idx_last_summary } else { idx_anchor };
                    let delta = if start_idx >= 0 { api_messages[(start_idx as usize + 1)..].to_vec() } else { Vec::new() };

                    let (delta_tokens, delta_count) = estimate_delta_stats(&delta);
                    let trigger_by_count = self.summary_threshold > 0 && delta_count >= self.summary_threshold;
                    let trigger_by_tokens = self.max_context_tokens > 0 && delta_tokens >= self.max_context_tokens;

                    if trigger_by_count || trigger_by_tokens {
                        let mut input = Vec::new();
                        if let Some(summary_prompt) = self.summary_system_prompt.clone() {
                            input.push(json!({"role": "system", "content": summary_prompt}));
                        } else if let Some(anchor) = self.anchor_user_content.clone() {
                            input.push(json!({"role": "user", "content": anchor}));
                        }
                        input.extend(delta);

                        let summary_callbacks = crate::services::v2::conversation_summarizer::SummaryCallbacks {
                            on_start: callbacks.on_context_summarized_start.clone(),
                            on_stream: callbacks.on_context_summarized_stream.clone().map(|cb| {
                                std::sync::Arc::new(move |chunk: String| {
                                    cb(Value::String(chunk));
                                }) as std::sync::Arc<dyn Fn(String) + Send + Sync>
                            }),
                            on_end: callbacks.on_context_summarized_end.clone(),
                        };

                        let res = summarizer.maybe_summarize_in_memory(
                            &input,
                            Some(SummaryOverrides {
                                message_limit: Some(1),
                                max_context_tokens: Some(1),
                                keep_last_n: Some(0),
                                target_summary_tokens: Some(self.target_summary_tokens),
                                model: Some(model.clone()),
                                temperature: Some(0.2),
                            }),
                            session_id.clone(),
                            true,
                            Some(summary_callbacks)
                        ).await;

                        match res {
                            Ok(res) => {
                                if res.summarized {
                                    self.summary_system_prompt = res.system_prompt.clone();
                                    let mut rebuilt = Vec::new();
                                    if let Some(prompt) = self.system_prompt.clone() { rebuilt.push(json!({"role": "system", "content": prompt})); }
                                    if let Some(anchor) = self.anchor_user_content.clone() {
                                        rebuilt.push(json!({"role": "user", "content": anchor}));
                                    }
                                    if let Some(summary_prompt) = self.summary_system_prompt.clone() {
                                        rebuilt.push(json!({"role": "system", "content": summary_prompt}));
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
            let attempt = self.ai_request_handler.handle_request(
                api_messages.clone(),
                tools.clone(),
                model.clone(),
                Some(temperature),
                max_tokens,
                StreamCallbacks { on_chunk: callbacks.on_chunk.clone(), on_thinking: callbacks.on_thinking.clone() },
                reasoning_enabled,
                provider.clone(),
                thinking_level.clone(),
                session_id.clone(),
                callbacks.on_chunk.is_some() || callbacks.on_thinking.is_some(),
                "chat",
            ).await;

            match attempt {
                Ok(r) => {
                    resp = Some(r);
                    break;
                }
                Err(err) => {
                    last_err = Some(err.clone());
                    if !token_limit_compacted && is_token_limit_error(&err) {
                        token_limit_compacted = true;
                        if let Some(compacted) = self.try_compact_for_token_limit(
                            &api_messages,
                            &err,
                            &callbacks,
                            session_id.clone(),
                            &model
                        ).await {
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
            None => return Err(last_err.unwrap_or_else(|| "request failed".to_string()))
        };

            let tool_calls = resp.tool_calls.clone();
            if tool_calls.is_none() || tool_calls.as_ref().unwrap().as_array().map(|a| a.is_empty()).unwrap_or(true) {
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
            if let Some(cb) = &callbacks.on_tools_start { cb(tool_calls_val.clone()); }
            let tool_calls_arr = tool_calls_val.as_array().cloned().unwrap_or_default();

            let build_aborted_results = |existing: Option<&Vec<ToolResult>>| -> Vec<ToolResult> {
                let mut results = existing.cloned().unwrap_or_default();
                let mut present: HashSet<String> = results.iter().map(|r| r.tool_call_id.clone()).collect();
                for tc in &tool_calls_arr {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if id.is_empty() || present.contains(&id) { continue; }
                    let name = tc.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .or_else(|| tc.get("name").and_then(|v| v.as_str()))
                        .unwrap_or("tool")
                        .to_string();
                    present.insert(id.clone());
                    results.push(ToolResult {
                        tool_call_id: id,
                        name,
                        success: false,
                        is_error: true,
                        content: "aborted".to_string(),
                    });
                }
                results
            };

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    let aborted_results = build_aborted_results(None);
                    for result in &aborted_results {
                        let meta = json!({
                            "toolName": result.name,
                            "success": result.success,
                            "isError": result.is_error
                        });
                        let _ = self.message_manager.save_tool_message(sid, &result.content, &result.tool_call_id, Some(meta)).await;
                    }
                    return Err("aborted".to_string());
                }
            }

            let on_tools_stream_cb = callbacks.on_tools_stream.clone().map(|cb| {
                let sid = session_id.clone();
                std::sync::Arc::new(move |result: &ToolResult| {
                    if let Some(ref sid) = sid {
                        if abort_registry::is_aborted(sid) { return; }
                    }
                    cb(serde_json::to_value(result).unwrap_or(json!({})));
                }) as std::sync::Arc<dyn Fn(&ToolResult) + Send + Sync>
            });

            let tool_results = self.mcp_tool_execute.execute_tools_stream(&tool_calls_arr, session_id.as_deref(), on_tools_stream_cb).await;

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    let aborted_results = build_aborted_results(Some(&tool_results));
                    for result in &aborted_results {
                        let meta = json!({
                            "toolName": result.name,
                            "success": result.success,
                            "isError": result.is_error
                        });
                        let _ = self.message_manager.save_tool_message(sid, &result.content, &result.tool_call_id, Some(meta)).await;
                    }
                    return Err("aborted".to_string());
                }
            }

            if let Some(cb) = &callbacks.on_tools_end {
                cb(json!({"tool_results": tool_results.clone()}));
            }

            if let Some(sid) = session_id.as_ref() {
                for result in &tool_results {
                    let meta = json!({
                        "toolName": result.name,
                        "success": result.success,
                        "isError": result.is_error
                    });
                    let _ = self.message_manager.save_tool_message(sid, &result.content, &result.tool_call_id, Some(meta)).await;
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
                assistant_msg["reasoning_content"] = Value::String(resp.reasoning.clone().unwrap_or_default());
            }
            if let Some(tc) = resp.tool_calls.clone() { assistant_msg["tool_calls"] = tc; }
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
        let summary_input_budget = if self.max_context_tokens > 0 { self.max_context_tokens } else { 6000 };

        if self.dynamic_summary_enabled {
            if self.summarizer.is_none() {
                self.summarizer = Some(ConversationSummarizer::new(self.ai_request_handler.clone(), self.message_manager.clone()));
            }
            if let Some(summarizer) = &self.summarizer {
                let (trimmed_for_summary, _) = truncate_messages_by_tokens(messages, summary_input_budget);
                let summary_callbacks = crate::services::v2::conversation_summarizer::SummaryCallbacks {
                    on_start: callbacks.on_context_summarized_start.clone(),
                    on_stream: callbacks.on_context_summarized_stream.clone().map(|cb| {
                        std::sync::Arc::new(move |chunk: String| {
                            cb(Value::String(chunk));
                        }) as std::sync::Arc<dyn Fn(String) + Send + Sync>
                    }),
                    on_end: callbacks.on_context_summarized_end.clone(),
                };

                let res = summarizer.maybe_summarize_in_memory(
                    &trimmed_for_summary,
                    Some(SummaryOverrides {
                        message_limit: Some(1),
                        max_context_tokens: Some(1),
                        keep_last_n: Some(0),
                        target_summary_tokens: Some(self.target_summary_tokens),
                        model: Some(model.to_string()),
                        temperature: Some(0.2),
                    }),
                    session_id.clone(),
                    true,
                    Some(summary_callbacks)
                ).await;

                match res {
                    Ok(res) => {
                        if res.summarized {
                            self.summary_system_prompt = res.system_prompt.clone();
                            let mut rebuilt = Vec::new();
                            if let Some(prompt) = self.system_prompt.clone() { rebuilt.push(json!({"role": "system", "content": prompt})); }
                            if let Some(anchor) = self.anchor_user_content.clone() { rebuilt.push(json!({"role": "user", "content": anchor})); }
                            if let Some(summary_prompt) = self.summary_system_prompt.clone() {
                                rebuilt.push(json!({"role": "system", "content": summary_prompt}));
                            }
                            if !rebuilt.is_empty() {
                                return Some(rebuilt);
                            }
                        }
                    }
                    Err(err) => {
                        warn!("[SUM-MEM] retry summary failed: {}", err);
                    }
                }
            }
        }

        let budget = token_limit_budget_from_error(err).unwrap_or(summary_input_budget).max(1000);
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
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) { self.max_iterations = v; }
        if let Some(v) = effective.get("DYNAMIC_SUMMARY_ENABLED").and_then(|v| v.as_bool()) { self.dynamic_summary_enabled = v; }
        if let Some(v) = effective.get("HISTORY_LIMIT").and_then(|v| v.as_i64()) { self.history_limit = v.max(0); }
        if let Some(v) = effective.get("SUMMARY_MESSAGE_LIMIT").and_then(|v| v.as_i64()) { self.summary_threshold = v; }
        if let Some(v) = effective.get("SUMMARY_KEEP_LAST_N").and_then(|v| v.as_i64()) { self.summary_keep_last_n = v; }
        if let Some(v) = effective.get("SUMMARY_MAX_CONTEXT_TOKENS").and_then(|v| v.as_i64()) { self.max_context_tokens = v; }
        if let Some(v) = effective.get("SUMMARY_TARGET_TOKENS").and_then(|v| v.as_i64()) { self.target_summary_tokens = v; }
    }
}

fn normalize_content(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        for part in arr {
            if part.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    return text.to_string();
                }
            }
        }
        return String::new();
    }
    content.to_string()
}

fn drop_duplicate_tail(history: Vec<Value>, current: &[Value]) -> Vec<Value> {
    if history.is_empty() || current.is_empty() {
        return history;
    }
    let mut i = history.len() as i64 - 1;
    let mut j = current.len() as i64 - 1;
    while i >= 0 && j >= 0 {
        let h = &history[i as usize];
        let c = &current[j as usize];
        if h.get("role") != c.get("role") {
            break;
        }
        let h_content = normalize_content(h.get("content").unwrap_or(&Value::Null));
        let c_content = normalize_content(c.get("content").unwrap_or(&Value::Null));
        if h_content != c_content {
            break;
        }
        i -= 1;
        j -= 1;
    }
    if j < (current.len() as i64 - 1) {
        if i < 0 {
            return Vec::new();
        }
        return history[..=(i as usize)].to_vec();
    }
    history
}

fn ensure_tool_responses(history: Vec<Value>) -> Vec<Value> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < history.len() {
        let msg = history[i].clone();
        if msg.get("role").and_then(|v| v.as_str()) == Some("tool") {
            i += 1;
            continue;
        }
        out.push(msg.clone());
        if msg.get("role").and_then(|v| v.as_str()) == Some("assistant") {
            let tool_calls = msg.get("tool_calls").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            if !tool_calls.is_empty() {
                let expected: Vec<String> = tool_calls.iter().filter_map(|tc| tc.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())).collect();
                let mut present = std::collections::HashSet::new();
                let mut j = i + 1;
                while j < history.len() {
                    let next = &history[j];
                    if next.get("role").and_then(|v| v.as_str()) != Some("tool") { break; }
                    if let Some(id) = next.get("tool_call_id").and_then(|v| v.as_str()) {
                        present.insert(id.to_string());
                    }
                    out.push(next.clone());
                    j += 1;
                }
                for id in expected {
                    if !present.contains(&id) {
                        out.push(json!({"role": "tool", "tool_call_id": id, "content": "aborted"}));
                    }
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn find_summary_index(messages: &[Value], summary_prompt: Option<&String>) -> i64 {
    if summary_prompt.is_none() { return -1; }
    let summary_prompt = summary_prompt.unwrap();
    for (idx, msg) in messages.iter().enumerate().rev() {
        if msg.get("role").and_then(|v| v.as_str()) == Some("system") {
            if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                if content == summary_prompt {
                    return idx as i64;
                }
            }
        }
    }
    -1
}

fn find_anchor_index(messages: &[Value], anchor: Option<&Value>) -> i64 {
    let anchor = match anchor { Some(a) => a, None => return -1 };
    for (idx, msg) in messages.iter().enumerate().rev() {
        if msg.get("role").and_then(|v| v.as_str()) == Some("user") {
            let content = msg.get("content").unwrap_or(&Value::Null);
            if content == anchor {
                return idx as i64;
            }
        }
    }
    -1
}

fn estimate_delta_stats(messages: &[Value]) -> (i64, i64) {
    let mut tokens = 0i64;
    let mut count = 0i64;
    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role != "system" && role != "user" {
            count += 1;
        }
        let content = msg.get("content").unwrap_or(&Value::Null);
        tokens += estimate_tokens_value(content);
    }
    (tokens, count)
}

fn estimate_tokens_plain(text: &str) -> i64 {
    if text.is_empty() { return 0; }
    ((text.len() as i64) + 3) / 4
}

fn estimate_tokens_value(content: &Value) -> i64 {
    if let Some(s) = content.as_str() {
        return estimate_tokens_plain(s);
    }
    if let Some(arr) = content.as_array() {
        let mut sum = 0i64;
        for part in arr {
            if let Some(s) = part.as_str() {
                sum += estimate_tokens_plain(s);
                continue;
            }
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                if let Some(ptype) = part.get("type").and_then(|v| v.as_str()) {
                    if ptype == "text" || ptype == "input_text" || ptype == "output_text" {
                        sum += estimate_tokens_plain(text);
                        continue;
                    }
                }
                sum += estimate_tokens_plain(text);
            }
        }
        return sum;
    }
    if let Some(obj) = content.as_object() {
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            return estimate_tokens_plain(text);
        }
        return estimate_tokens_plain(&content.to_string());
    }
    0
}

fn estimate_message_tokens(msg: &Value) -> i64 {
    let mut tokens = estimate_tokens_value(msg.get("content").unwrap_or(&Value::Null));
    if let Some(tc) = msg.get("tool_calls") {
        tokens += estimate_tokens_plain(&tc.to_string());
    }
    tokens
}

fn extract_error_message(err: &str) -> String {
    if let Ok(val) = serde_json::from_str::<Value>(err) {
        if let Some(msg) = val.get("error").and_then(|e| e.get("message")).and_then(|v| v.as_str()) {
            return msg.to_string();
        }
        if let Some(msg) = val.get("message").and_then(|v| v.as_str()) {
            return msg.to_string();
        }
    }
    err.to_string()
}

fn is_token_limit_error(err: &str) -> bool {
    let msg = extract_error_message(err).to_lowercase();
    msg.contains("token limit")
        || msg.contains("context length")
        || msg.contains("maximum context")
        || (msg.contains("exceeded") && msg.contains("token"))
}

fn parse_number_after(text: &str, key: &str) -> Option<i64> {
    let lower = text.to_lowercase();
    let idx = lower.find(key)?;
    let tail = &lower[idx + key.len()..];
    let digits: String = tail.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() { None } else { digits.parse().ok() }
}

fn token_limit_budget_from_error(err: &str) -> Option<i64> {
    let msg = extract_error_message(err);
    let lower = msg.to_lowercase();
    if !(lower.contains("token limit") || lower.contains("context length") || lower.contains("maximum context")) {
        return None;
    }
    let limit = parse_number_after(&msg, "limit")
        .or_else(|| parse_number_after(&msg, "context length"))
        .or_else(|| parse_number_after(&msg, "maximum context"));
    limit.map(|l| (l - 2048).max(1000))
}

fn truncate_messages_by_tokens(messages: &[Value], max_tokens: i64) -> (Vec<Value>, bool) {
    if max_tokens <= 0 || messages.is_empty() {
        return (messages.to_vec(), false);
    }

    let mut system_prefix = Vec::new();
    let mut idx = 0usize;
    while idx < messages.len() {
        if messages[idx].get("role").and_then(|v| v.as_str()) == Some("system") {
            system_prefix.push(messages[idx].clone());
            idx += 1;
            continue;
        }
        break;
    }

    let mut tokens: i64 = system_prefix.iter().map(estimate_message_tokens).sum();
    if tokens >= max_tokens {
        let truncated = truncate_messages_content_only(&system_prefix, max_tokens);
        return (truncated, true);
    }

    let mut tail_rev: Vec<Value> = Vec::new();
    for msg in messages[idx..].iter().rev() {
        let t = estimate_message_tokens(msg);
        if tokens + t > max_tokens {
            if tail_rev.is_empty() {
                let remaining = max_tokens - tokens;
                if remaining > 0 {
                    tail_rev.push(truncate_message_content(msg, remaining));
                }
            }
            break;
        }
        tokens += t;
        tail_rev.push(msg.clone());
    }
    tail_rev.reverse();

    let mut out = system_prefix;
    out.extend(tail_rev);
    let truncated = out.len() < messages.len();
    (out, truncated)
}

fn truncate_messages_content_only(messages: &[Value], max_tokens: i64) -> Vec<Value> {
    let mut out = Vec::new();
    let mut remaining = max_tokens;
    for msg in messages {
        if remaining <= 0 { break; }
        let t = estimate_message_tokens(msg);
        if t <= remaining {
            remaining -= t;
            out.push(msg.clone());
            continue;
        }
        out.push(truncate_message_content(msg, remaining));
        break;
    }
    out
}

fn truncate_message_content(msg: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 { return msg.clone(); }
    let mut out = msg.clone();
    if let Some(obj) = out.as_object_mut() {
        let content = obj.get("content").cloned().unwrap_or(Value::Null);
        let truncated = truncate_content_value(&content, max_tokens);
        obj.insert("content".to_string(), truncated);
    }
    out
}

fn truncate_content_value(content: &Value, max_tokens: i64) -> Value {
    if max_tokens <= 0 { return Value::String(String::new()); }
    if let Some(s) = content.as_str() {
        return Value::String(truncate_text_by_tokens(s, max_tokens));
    }
    if let Some(arr) = content.as_array() {
        let mut out = Vec::new();
        let mut remaining = max_tokens;
        for part in arr {
            if remaining <= 0 { break; }
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                let truncated = truncate_text_by_tokens(text, remaining);
                let used = estimate_tokens_plain(&truncated);
                let mut new_part = part.clone();
                if let Some(map) = new_part.as_object_mut() {
                    map.insert("text".to_string(), Value::String(truncated));
                }
                out.push(new_part);
                remaining -= used;
                continue;
            }
            if let Some(s) = part.as_str() {
                let truncated = truncate_text_by_tokens(s, remaining);
                let used = estimate_tokens_plain(&truncated);
                out.push(Value::String(truncated));
                remaining -= used;
                continue;
            }
            out.push(part.clone());
        }
        return Value::Array(out);
    }
    Value::String(truncate_text_by_tokens(&content.to_string(), max_tokens))
}

fn truncate_text_by_tokens(text: &str, max_tokens: i64) -> String {
    if max_tokens <= 0 { return String::new(); }
    let max_chars = (max_tokens * 4) as usize;
    if text.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let marker = "\n...[truncated]";
    if max_chars <= marker.len() {
        return marker[..max_chars].to_string();
    }
    let cut = max_chars - marker.len();
    format!("{}{}", &text[..cut], marker)
}

