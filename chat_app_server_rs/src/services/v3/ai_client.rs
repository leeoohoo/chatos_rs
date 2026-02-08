use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{Value, json};
use tracing::info;
use tracing::warn;

use crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v3::message_manager::MessageManager;
use crate::services::v3::mcp_tool_execute::{McpToolExecute, ToolResult};
use crate::services::user_settings::AiClientSettings;
use crate::utils::abort_registry;

#[derive(Clone)]
pub struct AiClientCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_tools_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
}

#[derive(Default)]
pub struct ProcessOptions {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub reasoning_enabled: Option<bool>,
    pub system_prompt: Option<String>,
    pub history_limit: Option<i64>,
    pub purpose: Option<String>,
    pub callbacks: Option<AiClientCallbacks>,
}

pub struct AiClient {
    ai_request_handler: AiRequestHandler,
    mcp_tool_execute: McpToolExecute,
    message_manager: MessageManager,
    max_iterations: i64,
    history_limit: i64,
    system_prompt: Option<String>,
    summary_threshold: i64,
    summary_keep_last_n: i64,
    max_context_tokens: Option<i64>,
    target_summary_tokens: Option<i64>,
    dynamic_summary_enabled: bool,
    prev_response_id_disabled_sessions: HashSet<String>,
    force_text_content_sessions: HashSet<String>,
}

impl AiClient {
    pub fn new(ai_request_handler: AiRequestHandler, mcp_tool_execute: McpToolExecute, message_manager: MessageManager) -> Self {
        Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            history_limit: 20,
            system_prompt: None,
            summary_threshold: 15,
            summary_keep_last_n: 2,
            max_context_tokens: None,
            target_summary_tokens: None,
            dynamic_summary_enabled: false,
            prev_response_id_disabled_sessions: HashSet::new(),
            force_text_content_sessions: HashSet::new(),
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
    }

    pub async fn process_request(
        &mut self,
        messages: Vec<Value>,
        session_id: Option<String>,
        options: ProcessOptions,
    ) -> Result<Value, String> {
        let model = options.model.unwrap_or_else(|| "gpt-4o".to_string());
        let provider = options.provider.unwrap_or_else(|| "gpt".to_string());
        let thinking_level = options.thinking_level.clone();
        let temperature = options.temperature.unwrap_or(0.7);
        let max_tokens = options.max_tokens;
        let reasoning_enabled = options.reasoning_enabled.unwrap_or(true);
        let system_prompt = options.system_prompt.or_else(|| self.system_prompt.clone());
        let history_limit = options.history_limit.unwrap_or(self.history_limit);
        let purpose = options.purpose.unwrap_or_else(|| "chat".to_string());
        let callbacks = options.callbacks.unwrap_or_else(|| AiClientCallbacks {
            on_chunk: None,
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
        });

        let mut previous_response_id: Option<String> = None;
        if let Some(sid) = session_id.as_ref() {
            if history_limit != 0 {
                let limit = if history_limit > 0 { Some(history_limit) } else { None };
                previous_response_id = self.message_manager.get_last_response_id(sid, limit.unwrap_or(50)).await;
            }
        }

        let raw_input = extract_raw_input(&messages);
        let force_text_content = session_id.as_ref().map(|s| self.force_text_content_sessions.contains(s)).unwrap_or(false);
        let available_tools = self.mcp_tool_execute.get_available_tools();
        let include_tool_items = !available_tools.is_empty();

        let allow_prev_id = session_id.as_ref().map(|s| !self.prev_response_id_disabled_sessions.contains(s)).unwrap_or(true);
        let provider_allows_prev = provider == "gpt" && base_url_allows_prev(self.ai_request_handler.base_url());
        let use_prev_id = previous_response_id.is_some() && allow_prev_id && provider_allows_prev;
        let stateless_history_limit = if !use_prev_id && history_limit == 0 {
            warn!("[AI_V3] history_limit=0 with stateless mode; fallback to 20");
            20
        } else {
            history_limit
        };
        info!(
            "[AI_V3] context mode: use_prev_id={}, provider={}, history_limit={}, has_prev_id={}",
            use_prev_id,
            provider,
            stateless_history_limit,
            previous_response_id.is_some()
        );
        let initial_input = if use_prev_id {
            normalize_input_for_provider(&raw_input, force_text_content)
        } else {
            let current_items = build_current_input_items(&raw_input, force_text_content);
            Value::Array(self.build_stateless_items(session_id.clone(), stateless_history_limit, force_text_content, &current_items, include_tool_items).await)
        };

        self.process_with_tools(
            initial_input,
            previous_response_id,
            available_tools,
            session_id,
            model,
            provider,
            thinking_level,
            temperature,
            max_tokens,
            callbacks,
            reasoning_enabled,
            system_prompt,
            &purpose,
            0,
            use_prev_id,
            raw_input,
            stateless_history_limit,
            force_text_content,
        ).await
    }

    async fn process_with_tools(
        &mut self,
        input: Value,
        previous_response_id: Option<String>,
        tools: Vec<Value>,
        session_id: Option<String>,
        model: String,
        provider: String,
        thinking_level: Option<String>,
        temperature: f64,
        max_tokens: Option<i64>,
        callbacks: AiClientCallbacks,
        reasoning_enabled: bool,
        system_prompt: Option<String>,
        purpose: &str,
        iteration: i64,
        use_prev_id: bool,
        raw_input: Value,
        history_limit: i64,
        force_text_content: bool,
    ) -> Result<Value, String> {
        let include_tool_items = !tools.is_empty();
        let mut input = input;
        let mut previous_response_id = previous_response_id;
        let mut use_prev_id = use_prev_id;
        let mut force_text_content = force_text_content;
        let mut iteration = iteration;
        let mut pending_tool_outputs: Option<Vec<Value>> = None;
        let mut pending_tool_calls: Option<Vec<Value>> = None;

        loop {
            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) { return Err("aborted".to_string()); }
            }
            if iteration >= self.max_iterations {
                return Err("达到最大迭代次数".to_string());
            }

            info!("AI_V3 request iteration {}", iteration);

            let mut ai_response = None;
            let mut last_error: Option<String> = None;

            for _attempt in 0..3 {
                let req = self.ai_request_handler.handle_request(
                    input.clone(),
                    model.clone(),
                    system_prompt.clone(),
                    if use_prev_id { previous_response_id.clone() } else { None },
                    if tools.is_empty() { None } else { Some(tools.clone()) },
                    Some(temperature),
                    max_tokens,
                    StreamCallbacks {
                        on_chunk: callbacks.on_chunk.clone(),
                        on_thinking: if reasoning_enabled { callbacks.on_thinking.clone() } else { None },
                    },
                    Some(provider.clone()),
                    thinking_level.clone(),
                    session_id.clone(),
                    callbacks.on_chunk.is_some() || callbacks.on_thinking.is_some(),
                    purpose,
                ).await;

                match req {
                    Ok(resp) => {
                        ai_response = Some(resp);
                        last_error = None;
                        break;
                    }
                    Err(err) => {
                        let err_msg = err.clone();
                        last_error = Some(err_msg.clone());
                        if use_prev_id && is_unsupported_previous_response_id_error(&err_msg) {
                            if let Some(sid) = session_id.as_ref() {
                                self.prev_response_id_disabled_sessions.insert(sid.clone());
                            }
                            let current_items = build_current_input_items(&raw_input, force_text_content);
                            let stateless = self.build_stateless_items(session_id.clone(), history_limit, force_text_content, &current_items, include_tool_items).await;
                            if !stateless.is_empty() {
                                use_prev_id = false;
                                previous_response_id = None;
                                input = Value::Array(stateless);
                                continue;
                            }
                        }
                        if use_prev_id && is_missing_tool_call_error(&err_msg) {
                            if let Some(sid) = session_id.as_ref() {
                                self.prev_response_id_disabled_sessions.insert(sid.clone());
                            }
                            let current_items = build_current_input_items(&raw_input, force_text_content);
                            let mut stateless = self.build_stateless_items(session_id.clone(), history_limit, force_text_content, &current_items, include_tool_items).await;
                            if include_tool_items {
                                let mut call_ids: HashSet<String> = HashSet::new();
                                if let Some(calls) = pending_tool_calls.as_ref() {
                                    for c in calls {
                                        if let Some(id) = c.get("call_id").and_then(|v| v.as_str()) {
                                            if !id.is_empty() { call_ids.insert(id.to_string()); }
                                        }
                                    }
                                    stateless.extend(calls.clone());
                                }
                                if let Some(outputs) = pending_tool_outputs.as_ref() {
                                    if call_ids.is_empty() {
                                        // no matching tool calls -> skip outputs to avoid invalid input
                                    } else {
                                        let filtered: Vec<Value> = outputs.iter().filter(|o| {
                                            o.get("call_id")
                                                .and_then(|v| v.as_str())
                                                .map(|id| call_ids.contains(id))
                                                .unwrap_or(false)
                                        }).cloned().collect();
                                        stateless.extend(filtered);
                                    }
                                }
                            }
                            if !stateless.is_empty() {
                                use_prev_id = false;
                                previous_response_id = None;
                                input = Value::Array(stateless);
                                continue;
                            }
                        }
                        if !force_text_content && is_invalid_input_text_error(&err_msg) {
                            force_text_content = true;
                            if let Some(sid) = session_id.as_ref() {
                                self.force_text_content_sessions.insert(sid.clone());
                            }
                            input = normalize_input_to_text_value(&input);
                            continue;
                        }
                        break;
                    }
                }
            }

            let ai_response = match ai_response {
                Some(resp) => resp,
                None => return Err(last_error.unwrap_or_else(|| "request failed".to_string())),
            };

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) { return Err("aborted".to_string()); }
            }

            let tool_calls = ai_response.tool_calls.clone();
            if tool_calls.as_ref().and_then(|v| v.as_array()).map(|a| a.is_empty()).unwrap_or(true) {
                return Ok(json!({
                    "success": true,
                    "content": ai_response.content,
                    "reasoning": ai_response.reasoning,
                    "tool_calls": Value::Null,
                    "finish_reason": ai_response.finish_reason,
                    "iteration": iteration
                }));
            }

            let tool_calls_val = tool_calls.unwrap_or(Value::Array(vec![]));
            if let Some(cb) = &callbacks.on_tools_start { cb(tool_calls_val.clone()); }
            let tool_calls_arr = tool_calls_val.as_array().cloned().unwrap_or_default();
            let tool_call_items = build_tool_call_items(&tool_calls_arr);

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

            let tool_results = self.mcp_tool_execute.execute_tools_stream(
                &tool_calls_arr,
                session_id.as_deref(),
                on_tools_stream_cb
            ).await;

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

            let tool_outputs: Vec<Value> = tool_results.iter().map(|r| {
                json!({
                    "type": "function_call_output",
                    "call_id": r.tool_call_id,
                    "output": r.content
                })
            }).collect();
            pending_tool_outputs = Some(tool_outputs.clone());
            pending_tool_calls = Some(tool_call_items.clone());

            let mut next_input = Value::Array(tool_outputs.clone());
            let mut next_prev_id = ai_response.response_id.clone();
            let mut next_use_prev_id = use_prev_id && next_prev_id.is_some();
            if use_prev_id && next_prev_id.is_none() {
                warn!("[AI_V3] missing response_id for tool call; fallback to stateless input");
                if let Some(sid) = session_id.as_ref() {
                    self.prev_response_id_disabled_sessions.insert(sid.clone());
                }
                next_use_prev_id = false;
            }

            if !next_use_prev_id {
                let current_items = build_current_input_items(&raw_input, force_text_content);
                let mut stateless = self.build_stateless_items(session_id.clone(), history_limit, force_text_content, &current_items, include_tool_items).await;
                if !ai_response.content.is_empty() {
                    stateless.push(to_message_item("assistant", &Value::String(ai_response.content.clone()), force_text_content));
                }
                if include_tool_items {
                    stateless.extend(tool_call_items);
                    stateless.extend(tool_outputs);
                }
                next_input = Value::Array(stateless);
                next_prev_id = None;
            }

            input = next_input;
            previous_response_id = next_prev_id;
            use_prev_id = next_use_prev_id;
            iteration += 1;
        }
    }

    async fn build_stateless_items(
        &self,
        session_id: Option<String>,
        history_limit: i64,
        force_text: bool,
        current_input_items: &[Value],
        include_tool_items: bool,
    ) -> Vec<Value> {
        let mut items = Vec::new();
        let mut summary_count = 0usize;
        let mut history_count = 0usize;
        let mut tool_call_ids: HashSet<String> = HashSet::new();
        let mut tool_output_ids: HashSet<String> = HashSet::new();
        if let Some(sid) = session_id.as_ref() {
            if history_limit != 0 {
                let limit = if history_limit > 0 { Some(history_limit) } else { None };
                let summary_limit = Some(2);
                let (summaries, history) = self.message_manager.get_session_history_with_summaries(sid, limit, summary_limit).await;
                let has_summary_table = !summaries.is_empty();
                summary_count = summaries.len();
                history_count = history.len();
                if has_summary_table {
                    for summary in summaries {
                        if !summary.summary_text.is_empty() {
                            let content = format!("以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}", summary.summary_text);
                            items.push(to_message_item("system", &Value::String(content), force_text));
                        }
                    }
                }
                if include_tool_items {
                    for msg in &history {
                        if msg.role == "tool" {
                            if let Some(call_id) = msg.tool_call_id.clone() {
                                if !call_id.is_empty() { tool_output_ids.insert(call_id); }
                            }
                        }
                    }
                }
                for msg in history {
                    if msg.metadata.as_ref().and_then(|m| m.get("type")).and_then(|v| v.as_str()) == Some("session_summary") {
                        if has_summary_table { continue; }
                        if let Some(summary) = msg.summary.clone() {
                            let content = format!("以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}", summary);
                            items.push(to_message_item("system", &Value::String(content), force_text));
                        }
                        continue;
                    }
                    if msg.role == "user" || msg.role == "assistant" || msg.role == "system" || msg.role == "developer" {
                        items.push(to_message_item(&msg.role, &Value::String(msg.content.clone()), force_text));
                        if include_tool_items {
                            let mut tool_calls = msg.tool_calls.clone().or_else(|| msg.metadata.as_ref().and_then(|m| m.get("toolCalls").cloned()));
                            if let Some(Value::String(s)) = tool_calls.clone() {
                                if let Ok(v) = serde_json::from_str::<Value>(&s) {
                                    tool_calls = Some(v);
                                }
                            }
                            if msg.role == "assistant" {
                                if let Some(arr) = tool_calls.and_then(|v| v.as_array().cloned()) {
                                    for tc in arr {
                                        let call_id = tc.get("id")
                                            .and_then(|v| v.as_str())
                                            .or_else(|| tc.get("call_id").and_then(|v| v.as_str()))
                                            .unwrap_or("")
                                            .to_string();
                                        if call_id.is_empty() { continue; }
                                        if !tool_output_ids.contains(&call_id) { continue; }
                                        let func = tc.get("function").cloned().unwrap_or(json!({}));
                                        let name = func.get("name").and_then(|v| v.as_str()).or_else(|| tc.get("name").and_then(|v| v.as_str())).unwrap_or("").to_string();
                                        let args = func.get("arguments").cloned().or_else(|| tc.get("arguments").cloned()).unwrap_or(Value::String("{}".to_string()));
                                        let args_str = if let Some(s) = args.as_str() { s.to_string() } else { args.to_string() };
                                        tool_call_ids.insert(call_id.clone());
                                        items.push(json!({
                                            "type": "function_call",
                                            "call_id": call_id,
                                            "name": name,
                                            "arguments": args_str
                                        }));
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    if msg.role == "tool" {
                        if include_tool_items {
                            if let Some(call_id) = msg.tool_call_id.clone() {
                                if tool_call_ids.contains(&call_id) {
                                    let output = msg.content.clone();
                                    items.push(json!({
                                        "type": "function_call_output",
                                        "call_id": call_id,
                                        "output": output
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(last) = items.last() {
            if last.get("type").and_then(|v| v.as_str()) == Some("message")
                && last.get("role").and_then(|v| v.as_str()) == Some("user")
            {
                items.pop();
            }
        }
        items.extend_from_slice(current_input_items);
        info!(
            "[AI_V3] stateless items built: summaries={}, history_messages={}, total_items={}",
            summary_count,
            history_count,
            items.len()
        );
        items
    }
}

fn extract_raw_input(messages: &[Value]) -> Value {
    if let Some(last_user) = messages.iter().rev().find(|m| m.get("role").and_then(|v| v.as_str()) == Some("user")) {
        if let Some(content) = last_user.get("content") {
            return convert_parts_to_response_input(content);
        }
    }
    if let Some(last) = messages.last() {
        if let Some(content) = last.get("content") {
            return convert_parts_to_response_input(content);
        }
    }
    Value::String(String::new())
}

fn convert_parts_to_response_input(content: &Value) -> Value {
    if let Some(s) = content.as_str() {
        return Value::String(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut content_list = Vec::new();
        for part in arr {
            if let Some(ptype) = part.get("type").and_then(|v| v.as_str()) {
                if ptype == "input_text" {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        content_list.push(json!({"type": "input_text", "text": text}));
                        continue;
                    }
                }
                if ptype == "input_image" {
                    let image_url = part.get("image_url").cloned().unwrap_or(Value::Null);
                    let file_id = part.get("file_id").cloned().unwrap_or(Value::Null);
                    let detail = part.get("detail").cloned().unwrap_or(Value::String("auto".to_string()));
                    content_list.push(json!({"type": "input_image", "image_url": image_url, "file_id": file_id, "detail": detail}));
                    continue;
                }
                if ptype == "text" {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        content_list.push(json!({"type": "input_text", "text": text}));
                        continue;
                    }
                }
                if ptype == "image_url" {
                    let url = part.get("image_url").and_then(|v| v.get("url")).and_then(|v| v.as_str())
                        .or_else(|| part.get("image_url").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    content_list.push(json!({"type": "input_image", "image_url": url, "detail": part.get("detail").cloned().unwrap_or(Value::String("auto".to_string()))}));
                    continue;
                }
            }
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                content_list.push(json!({"type": "input_text", "text": text}));
                continue;
            }
            content_list.push(json!({"type": "input_text", "text": part.to_string()}));
        }
        return Value::Array(vec![json!({"role": "user", "content": content_list, "type": "message"})]);
    }
    Value::String(content.to_string())
}

fn to_message_item(role: &str, content: &Value, force_text_content: bool) -> Value {
    if force_text_content {
        return json!({"role": role, "content": content_parts_to_text(content), "type": "message"});
    }
    if role == "assistant" {
        return json!({"role": role, "content": [ {"type": "output_text", "text": content_parts_to_text(content)} ], "type": "message"});
    }
    if content.is_array() {
        return json!({"role": role, "content": content.clone(), "type": "message"});
    }
    json!({"role": role, "content": to_input_text_content(content_parts_to_text(content)), "type": "message"})
}

fn to_input_text_content(text: String) -> Value {
    Value::Array(vec![json!({"type": "input_text", "text": text})])
}

fn content_parts_to_text(content: &Value) -> String {
    if let Some(s) = content.as_str() { return s.to_string(); }
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for part in arr {
            if let Some(s) = part.as_str() {
                parts.push(s.to_string());
                continue;
            }
            if let Some(ptype) = part.get("type").and_then(|v| v.as_str()) {
                if (ptype == "input_text" || ptype == "output_text" || ptype == "text") && part.get("text").and_then(|v| v.as_str()).is_some() {
                    parts.push(part.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string());
                    continue;
                }
                if ptype == "input_image" || ptype == "image_url" {
                    let url = part.get("image_url").and_then(|v| v.get("url")).and_then(|v| v.as_str())
                        .or_else(|| part.get("image_url").and_then(|v| v.as_str()))
                        .or_else(|| part.get("file_id").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    parts.push(if url.is_empty() { "[image]".to_string() } else { format!("[image:{}]", url) });
                    continue;
                }
            }
            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                parts.push(t.to_string());
                continue;
            }
            parts.push(part.to_string());
        }
        return parts.join("\n");
    }
    content.to_string()
}

fn normalize_input_to_text_value(input: &Value) -> Value {
    if let Some(arr) = input.as_array() {
        let mapped: Vec<Value> = arr.iter().map(|item| {
            if item.get("type").and_then(|v| v.as_str()) == Some("message") {
                let content = item.get("content").cloned().unwrap_or(Value::Null);
                let mut obj = item.clone();
                if let Some(map) = obj.as_object_mut() {
                    map.insert("content".to_string(), Value::String(content_parts_to_text(&content)));
                }
                return obj;
            }
            item.clone()
        }).collect();
        return Value::Array(mapped);
    }
    input.clone()
}

fn normalize_input_for_provider(input: &Value, force_text: bool) -> Value {
    if force_text {
        normalize_input_to_text_value(input)
    } else {
        input.clone()
    }
}

fn build_current_input_items(raw_input: &Value, force_text: bool) -> Vec<Value> {
    let normalized = normalize_input_for_provider(raw_input, force_text);
    if let Some(arr) = normalized.as_array() {
        return arr.clone();
    }
    vec![to_message_item("user", &normalized, force_text)]
}

fn build_tool_call_items(tool_calls_arr: &[Value]) -> Vec<Value> {
    let mut items = Vec::new();
    for tc in tool_calls_arr {
        let call_id = tc.get("id")
            .and_then(|v| v.as_str())
            .or_else(|| tc.get("call_id").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        if call_id.is_empty() { continue; }
        let func = tc.get("function").cloned().unwrap_or(json!({}));
        let name = func.get("name").and_then(|v| v.as_str())
            .or_else(|| tc.get("name").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let args = func.get("arguments").cloned()
            .or_else(|| tc.get("arguments").cloned())
            .unwrap_or(Value::String("{}".to_string()));
        let args_str = if let Some(s) = args.as_str() { s.to_string() } else { args.to_string() };
        items.push(json!({
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": args_str
        }));
    }
    items
}

fn is_unsupported_previous_response_id_error(err: &str) -> bool {
    let msg = err.to_lowercase();
    msg.contains("previous_response_id") && (msg.contains("unsupported parameter") || msg.contains("invalid parameter"))
}

fn base_url_allows_prev(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("api.openai.com") {
        return true;
    }
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }
    if let Ok(val) = std::env::var("ALLOW_PREV_ID_FOR_PROXY") {
        let v = val.trim().to_lowercase();
        if v == "1" || v == "true" || v == "yes" || v == "on" {
            return true;
        }
    }
    false
}

fn is_invalid_input_text_error(err: &str) -> bool {
    let msg = err.to_lowercase();
    msg.contains("input_text") && (msg.contains("invalid value") || msg.contains("invalid_value"))
}

fn is_missing_tool_call_error(err: &str) -> bool {
    let msg = err.to_lowercase();
    msg.contains("no tool call found")
        && (msg.contains("function call output") || msg.contains("function_call_output"))
}

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) { self.max_iterations = v; }
        if let Some(v) = effective.get("HISTORY_LIMIT").and_then(|v| v.as_i64()) { self.history_limit = v.max(0); }
        if let Some(v) = effective.get("SUMMARY_MESSAGE_LIMIT").and_then(|v| v.as_i64()) { self.summary_threshold = v; }
        if let Some(v) = effective.get("SUMMARY_KEEP_LAST_N").and_then(|v| v.as_i64()) { self.summary_keep_last_n = v; }
        if let Some(v) = effective.get("SUMMARY_MAX_CONTEXT_TOKENS").and_then(|v| v.as_i64()) { self.max_context_tokens = Some(v); }
        if let Some(v) = effective.get("SUMMARY_TARGET_TOKENS").and_then(|v| v.as_i64()) { self.target_summary_tokens = Some(v); }
        if let Some(v) = effective.get("DYNAMIC_SUMMARY_ENABLED").and_then(|v| v.as_bool()) { self.dynamic_summary_enabled = v; }
    }
}
