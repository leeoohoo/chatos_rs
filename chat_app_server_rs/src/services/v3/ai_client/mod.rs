use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{json, Value};
use tracing::info;
use tracing::warn;

use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::user_settings::AiClientSettings;
use crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;
use crate::utils::abort_registry;

mod input_transform;
mod prev_context;
mod tool_plan;

use self::input_transform::{
    build_current_input_items, extract_raw_input, normalize_input_for_provider,
    normalize_input_to_text_value, to_message_item,
};
use self::prev_context::{
    base_url_allows_prev, base_url_disallows_system_messages, is_context_length_exceeded_error,
    is_input_must_be_list_error, is_invalid_input_text_error, is_missing_tool_call_error,
    is_system_messages_not_allowed_error, is_unsupported_previous_response_id_error,
    reduce_history_limit, should_use_prev_id_for_next_turn,
};
use self::tool_plan::{
    build_tool_call_execution_plan, build_tool_call_items, expand_tool_results_with_aliases,
};

#[derive(Clone)]
pub struct AiClientCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_tools_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
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
    pub conversation_turn_id: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub sub_agent_run_id: Option<String>,
    pub callbacks: Option<AiClientCallbacks>,
}

pub struct AiClient {
    ai_request_handler: AiRequestHandler,
    mcp_tool_execute: McpToolExecute,
    message_manager: MessageManager,
    max_iterations: i64,
    history_limit: i64,
    system_prompt: Option<String>,
    prev_response_id_disabled_sessions: HashSet<String>,
    force_text_content_sessions: HashSet<String>,
    no_system_message_sessions: HashSet<String>,
}

impl AiClient {
    pub fn new(
        ai_request_handler: AiRequestHandler,
        mcp_tool_execute: McpToolExecute,
        message_manager: MessageManager,
    ) -> Self {
        Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            history_limit: 20,
            system_prompt: None,
            prev_response_id_disabled_sessions: HashSet::new(),
            force_text_content_sessions: HashSet::new(),
            no_system_message_sessions: HashSet::new(),
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
        let message_mode = options.message_mode;
        let message_source = options.message_source;
        let sub_agent_run_id = options
            .sub_agent_run_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let turn_id = options
            .conversation_turn_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let callbacks = options.callbacks.unwrap_or_else(|| AiClientCallbacks {
            on_chunk: None,
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        });
        let stable_prefix_mode = purpose == "chat" || sub_agent_run_id.is_some();

        // Chat mode favors a stable stateless prefix so provider-side prompt caching can reuse
        // a bounded recent window even when async summary jobs are updating session summaries.
        let prefer_stateless = if purpose == "chat" || sub_agent_run_id.is_some() {
            true
        } else {
            history_limit != 0
        };
        let mut previous_response_id: Option<String> = None;
        if !prefer_stateless {
            if let Some(sid) = session_id.as_ref() {
                let limit = if history_limit > 0 {
                    Some(history_limit)
                } else {
                    None
                };
                previous_response_id = self
                    .message_manager
                    .get_last_response_id(sid, limit.unwrap_or(50))
                    .await;
            }
        }

        let raw_input = extract_raw_input(&messages);
        let force_text_content = session_id
            .as_ref()
            .map(|s| self.force_text_content_sessions.contains(s))
            .unwrap_or(false);
        let available_tools = self.mcp_tool_execute.get_available_tools();
        let include_tool_items = !available_tools.is_empty();

        let allow_prev_id = session_id
            .as_ref()
            .map(|s| !self.prev_response_id_disabled_sessions.contains(s))
            .unwrap_or(true);
        let provider_allows_prev =
            provider == "gpt" && base_url_allows_prev(self.ai_request_handler.base_url());
        let can_use_prev_id = allow_prev_id && provider_allows_prev;
        let use_prev_id = !prefer_stateless && previous_response_id.is_some() && can_use_prev_id;
        let stateless_history_limit = if !use_prev_id && history_limit == 0 {
            warn!("[AI_V3] history_limit=0 with stateless mode; fallback to 20");
            20
        } else {
            history_limit
        };
        info!(
            "[AI_V3] context mode: use_prev_id={}, can_use_prev_id={}, provider={}, history_limit={}, has_prev_id={}, stable_prefix_mode={}",
            use_prev_id,
            can_use_prev_id,
            provider,
            stateless_history_limit,
            previous_response_id.is_some(),
            stable_prefix_mode
        );
        let initial_input = if use_prev_id {
            normalize_input_for_provider(&raw_input, force_text_content)
        } else {
            let current_items = build_current_input_items(&raw_input, force_text_content);
            Value::Array(
                self.build_stateless_items(
                    session_id.clone(),
                    stateless_history_limit,
                    stable_prefix_mode,
                    force_text_content,
                    &current_items,
                    include_tool_items,
                    sub_agent_run_id.clone(),
                )
                .await,
            )
        };

        let result = self
            .process_with_tools(
                initial_input,
                previous_response_id,
                available_tools,
                session_id,
                turn_id,
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
                can_use_prev_id,
                raw_input,
                stateless_history_limit,
                stable_prefix_mode,
                force_text_content,
                prefer_stateless,
                message_mode,
                message_source,
                sub_agent_run_id,
            )
            .await;

        result
    }

    async fn maybe_refresh_stateless_context(
        &self,
        session_id: Option<&str>,
        sub_agent_run_id: Option<&str>,
        stable_prefix_mode: bool,
        use_prev_id: bool,
        raw_input: &Value,
        force_text_content: bool,
        history_limit: i64,
        include_tool_items: bool,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
    ) {
        if !stable_prefix_mode || use_prev_id {
            return;
        }

        if session_id.is_none() && sub_agent_run_id.is_none() {
            return;
        }

        let current_items = build_current_input_items(raw_input, force_text_content);
        let rebuilt = self
            .build_stateless_items(
                session_id.map(|value| value.to_string()),
                history_limit,
                stable_prefix_mode,
                force_text_content,
                &current_items,
                include_tool_items,
                sub_agent_run_id.map(|value| value.to_string()),
            )
            .await;
        let previous_len = stateless_context_items
            .as_ref()
            .map(|items| items.len())
            .unwrap_or(0);
        let changed = stateless_context_items
            .as_ref()
            .map(|items| items != &rebuilt)
            .unwrap_or(true);
        if changed {
            info!(
                "[AI_V3] stateless context refreshed: old_items={}, new_items={}, history_limit={}",
                previous_len,
                rebuilt.len(),
                history_limit
            );
            *stateless_context_items = Some(rebuilt.clone());
            *input = Value::Array(rebuilt);
        }
    }

    async fn process_with_tools(
        &mut self,
        input: Value,
        previous_response_id: Option<String>,
        tools: Vec<Value>,
        session_id: Option<String>,
        turn_id: Option<String>,
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
        can_use_prev_id: bool,
        raw_input: Value,
        history_limit: i64,
        stable_prefix_mode: bool,
        force_text_content: bool,
        prefer_stateless: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
        sub_agent_run_id: Option<String>,
    ) -> Result<Value, String> {
        let include_tool_items = !tools.is_empty();
        let persist_tool_messages = purpose != "sub_agent_router";
        let mut input = input;
        let mut previous_response_id = previous_response_id;
        let mut use_prev_id = use_prev_id;
        let mut can_use_prev_id = can_use_prev_id;
        let mut force_text_content = force_text_content;
        let mut adaptive_history_limit = history_limit;
        let mut iteration = iteration;
        let mut pending_tool_outputs: Option<Vec<Value>> = None;
        let mut pending_tool_calls: Option<Vec<Value>> = None;
        let mut no_system_messages =
            base_url_disallows_system_messages(self.ai_request_handler.base_url())
                || session_id
                    .as_ref()
                    .map(|sid| self.no_system_message_sessions.contains(sid))
                    .unwrap_or(false);
        let mut stateless_context_items = if !use_prev_id {
            input.as_array().cloned()
        } else {
            None
        };

        loop {
            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    return Err("aborted".to_string());
                }
            }
            if iteration >= self.max_iterations {
                return Err("达到最大迭代次数".to_string());
            }

            info!("AI_V3 request iteration {}", iteration);

            // In chat/stateless mode, refresh context from persisted summary+pending messages
            // before each model request so newly generated summaries are reflected immediately.
            self.maybe_refresh_stateless_context(
                session_id.as_deref(),
                sub_agent_run_id.as_deref(),
                stable_prefix_mode,
                use_prev_id,
                &raw_input,
                force_text_content,
                adaptive_history_limit,
                include_tool_items,
                &mut stateless_context_items,
                &mut input,
            )
            .await;

            let mut ai_response = None;
            let mut last_error: Option<String> = None;

            for _attempt in 0..3 {
                let request_input = if no_system_messages {
                    rewrite_system_messages_to_user(&input, force_text_content)
                } else {
                    input.clone()
                };
                let req = self
                    .ai_request_handler
                    .handle_request(
                        request_input,
                        model.clone(),
                        system_prompt.clone(),
                        if use_prev_id {
                            previous_response_id.clone()
                        } else {
                            None
                        },
                        if tools.is_empty() {
                            None
                        } else {
                            Some(tools.clone())
                        },
                        Some(temperature),
                        max_tokens,
                        StreamCallbacks {
                            on_chunk: callbacks.on_chunk.clone(),
                            on_thinking: if reasoning_enabled {
                                callbacks.on_thinking.clone()
                            } else {
                                None
                            },
                        },
                        Some(provider.clone()),
                        thinking_level.clone(),
                        session_id.clone(),
                        turn_id.clone(),
                        callbacks.on_chunk.is_some() || callbacks.on_thinking.is_some(),
                        message_mode.clone(),
                        message_source.clone(),
                        purpose,
                    )
                    .await;

                match req {
                    Ok(resp) => {
                        ai_response = Some(resp);
                        last_error = None;
                        break;
                    }
                    Err(err) => {
                        let err_msg = err.clone();
                        last_error = Some(err_msg.clone());
                        if !no_system_messages && is_system_messages_not_allowed_error(&err_msg) {
                            warn!(
                                "[AI_V3] provider rejected system-role input; retry with user-role compatibility mode"
                            );
                            no_system_messages = true;
                            if let Some(sid) = session_id.as_ref() {
                                self.no_system_message_sessions.insert(sid.clone());
                            }
                            continue;
                        }
                        if use_prev_id && is_unsupported_previous_response_id_error(&err_msg) {
                            if let Some(sid) = session_id.as_ref() {
                                self.prev_response_id_disabled_sessions.insert(sid.clone());
                            }
                            warn!("[AI_V3] previous_response_id unsupported; fallback to stateless mode");
                            can_use_prev_id = false;
                            let current_items =
                                build_current_input_items(&raw_input, force_text_content);
                            let stateless = self
                                .build_stateless_items(
                                    session_id.clone(),
                                    adaptive_history_limit,
                                    stable_prefix_mode,
                                    force_text_content,
                                    &current_items,
                                    include_tool_items,
                                    sub_agent_run_id.clone(),
                                )
                                .await;
                            if !stateless.is_empty() {
                                use_prev_id = false;
                                previous_response_id = None;
                                stateless_context_items = Some(stateless.clone());
                                input = Value::Array(stateless);
                                continue;
                            }
                        }
                        if use_prev_id && is_missing_tool_call_error(&err_msg) {
                            if let Some(sid) = session_id.as_ref() {
                                self.prev_response_id_disabled_sessions.insert(sid.clone());
                            }
                            warn!(
                                "[AI_V3] function_call_output missing matching tool call in previous response; fallback to stateless mode"
                            );
                            can_use_prev_id = false;
                            let current_items =
                                build_current_input_items(&raw_input, force_text_content);
                            let mut stateless = if let Some(items) = stateless_context_items.clone()
                            {
                                items
                            } else {
                                self.build_stateless_items(
                                    session_id.clone(),
                                    adaptive_history_limit,
                                    stable_prefix_mode,
                                    force_text_content,
                                    &current_items,
                                    include_tool_items,
                                    sub_agent_run_id.clone(),
                                )
                                .await
                            };
                            if include_tool_items {
                                let mut call_ids: HashSet<String> = HashSet::new();
                                let mut existing_call_ids: HashSet<String> = stateless
                                    .iter()
                                    .filter(|item| {
                                        item.get("type").and_then(|v| v.as_str())
                                            == Some("function_call")
                                    })
                                    .filter_map(|item| {
                                        item.get("call_id")
                                            .and_then(|v| v.as_str())
                                            .map(|value| value.to_string())
                                    })
                                    .collect();
                                let mut existing_output_ids: HashSet<String> = stateless
                                    .iter()
                                    .filter(|item| {
                                        item.get("type").and_then(|v| v.as_str())
                                            == Some("function_call_output")
                                    })
                                    .filter_map(|item| {
                                        item.get("call_id")
                                            .and_then(|v| v.as_str())
                                            .map(|value| value.to_string())
                                    })
                                    .collect();
                                if let Some(calls) = pending_tool_calls.as_ref() {
                                    for c in calls {
                                        if let Some(id) = c.get("call_id").and_then(|v| v.as_str())
                                        {
                                            if !id.is_empty() {
                                                call_ids.insert(id.to_string());
                                                if existing_call_ids.insert(id.to_string()) {
                                                    stateless.push(c.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Some(outputs) = pending_tool_outputs.as_ref() {
                                    if call_ids.is_empty() {
                                        // no matching tool calls -> skip outputs to avoid invalid input
                                    } else {
                                        for output in outputs {
                                            let Some(id) = output
                                                .get("call_id")
                                                .and_then(|v| v.as_str())
                                                .map(|value| value.to_string())
                                            else {
                                                continue;
                                            };
                                            if !call_ids.contains(id.as_str()) {
                                                continue;
                                            }
                                            if existing_output_ids.insert(id) {
                                                stateless.push(output.clone());
                                            }
                                        }
                                    }
                                }
                            }
                            if !stateless.is_empty() {
                                use_prev_id = false;
                                previous_response_id = None;
                                stateless_context_items = Some(stateless.clone());
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
                        if is_input_must_be_list_error(&err_msg) {
                            warn!("[AI_V3] provider requires list input; retry with message-list payload");
                            let normalized_items = if let Some(items) = input.as_array() {
                                items.clone()
                            } else {
                                build_current_input_items(&input, force_text_content)
                            };
                            input = Value::Array(normalized_items.clone());
                            stateless_context_items = Some(normalized_items);
                            continue;
                        }
                        if is_context_length_exceeded_error(&err_msg) {
                            if let Some(next_limit) = reduce_history_limit(adaptive_history_limit) {
                                warn!(
                                    "[AI_V3] context length exceeded; reduce history_limit {} -> {}",
                                    adaptive_history_limit,
                                    next_limit
                                );
                                adaptive_history_limit = next_limit;
                                can_use_prev_id = false;
                                let current_items =
                                    build_current_input_items(&raw_input, force_text_content);
                                let stateless = self
                                    .build_stateless_items(
                                        session_id.clone(),
                                        adaptive_history_limit,
                                        stable_prefix_mode,
                                        force_text_content,
                                        &current_items,
                                        include_tool_items,
                                        sub_agent_run_id.clone(),
                                    )
                                    .await;
                                if !stateless.is_empty() {
                                    use_prev_id = false;
                                    previous_response_id = None;
                                    stateless_context_items = Some(stateless.clone());
                                    input = Value::Array(stateless);
                                    continue;
                                }
                            }
                        }
                        break;
                    }
                }
            }

            let ai_response = match ai_response {
                Some(resp) => resp,
                None => return Err(last_error.unwrap_or_else(|| "request failed".to_string())),
            };
            log_usage_snapshot(purpose, ai_response.usage.as_ref());

            if let Some(err) = completion_failed_error(
                ai_response.finish_reason.as_deref(),
                ai_response.content.as_str(),
                ai_response.reasoning.as_deref(),
                ai_response.provider_error.as_ref(),
            ) {
                if is_context_length_exceeded_error(&err) {
                    if let Some(next_limit) = reduce_history_limit(adaptive_history_limit) {
                        warn!(
                            "[AI_V3] failed response due to context overflow; reduce history_limit {} -> {}",
                            adaptive_history_limit,
                            next_limit
                        );
                        adaptive_history_limit = next_limit;
                        can_use_prev_id = false;
                        use_prev_id = false;
                        previous_response_id = None;
                        let current_items =
                            build_current_input_items(&raw_input, force_text_content);
                        let stateless = self
                            .build_stateless_items(
                                session_id.clone(),
                                adaptive_history_limit,
                                stable_prefix_mode,
                                force_text_content,
                                &current_items,
                                include_tool_items,
                                sub_agent_run_id.clone(),
                            )
                            .await;
                        if !stateless.is_empty() {
                            stateless_context_items = Some(stateless.clone());
                            input = Value::Array(stateless);
                            continue;
                        }
                    }
                }
                return Err(err);
            }

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    return Err("aborted".to_string());
                }
            }

            let tool_calls = ai_response.tool_calls.clone();
            if tool_calls
                .as_ref()
                .and_then(|v| v.as_array())
                .map(|a| a.is_empty())
                .unwrap_or(true)
            {
                return Ok(json!({
                    "success": true,
                    "content": ai_response.content,
                    "reasoning": ai_response.reasoning,
                    "tool_calls": Value::Null,
                    "finish_reason": ai_response.finish_reason,
                    "iteration": iteration
                }));
            }

            let raw_tool_calls = tool_calls.unwrap_or(Value::Array(vec![]));
            let tool_calls_arr = raw_tool_calls.as_array().cloned().unwrap_or_default();
            let execution_plan = build_tool_call_execution_plan(&tool_calls_arr);
            let display_tool_calls = Value::Array(execution_plan.display_calls.clone());

            if let Some(cb) = &callbacks.on_tools_start {
                cb(display_tool_calls);
            }
            let tool_call_items = build_tool_call_items(&tool_calls_arr);

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
                    &execution_plan.execute_calls,
                    session_id.as_deref(),
                    turn_id.as_deref(),
                    Some(model.as_str()),
                    on_tools_stream_cb,
                )
                .await;
            let expanded_tool_results = expand_tool_results_with_aliases(
                tool_results.as_slice(),
                &execution_plan.alias_map,
            );

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    if persist_tool_messages {
                        let aborted_results = build_aborted_tool_results(
                            &tool_calls_arr,
                            Some(expanded_tool_results.as_slice()),
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
                        .save_tool_results(sid, expanded_tool_results.as_slice())
                        .await;
                }
            }

            let tool_outputs: Vec<Value> = expanded_tool_results
                .iter()
                .map(|r| {
                    json!({
                        "type": "function_call_output",
                        "call_id": r.tool_call_id,
                        "output": r.content
                    })
                })
                .collect();
            pending_tool_outputs = Some(tool_outputs.clone());
            pending_tool_calls = Some(tool_call_items.clone());

            let assistant_item = if !ai_response.content.is_empty() {
                Some(to_message_item(
                    "assistant",
                    &Value::String(ai_response.content.clone()),
                    force_text_content,
                ))
            } else {
                None
            };

            if let Some(items) = stateless_context_items.as_mut() {
                if let Some(item) = assistant_item.clone() {
                    items.push(item);
                }
                if include_tool_items {
                    items.extend(tool_call_items.clone());
                    items.extend(tool_outputs.clone());
                }
            }

            let mut next_input = Value::Array(tool_outputs.clone());
            let mut next_prev_id = ai_response.response_id.clone();
            let mut next_use_prev_id = should_use_prev_id_for_next_turn(
                prefer_stateless,
                can_use_prev_id,
                next_prev_id.is_some(),
            );
            if use_prev_id && next_prev_id.is_none() {
                warn!("[AI_V3] missing response_id for tool call; fallback to stateless input");
                if let Some(sid) = session_id.as_ref() {
                    self.prev_response_id_disabled_sessions.insert(sid.clone());
                }
                can_use_prev_id = false;
                next_use_prev_id = false;
            }

            if !next_use_prev_id {
                let mut stateless = if let Some(items) = stateless_context_items.clone() {
                    items
                } else {
                    let current_items = build_current_input_items(&raw_input, force_text_content);
                    self.build_stateless_items(
                        session_id.clone(),
                        adaptive_history_limit,
                        stable_prefix_mode,
                        force_text_content,
                        &current_items,
                        include_tool_items,
                        sub_agent_run_id.clone(),
                    )
                    .await
                };

                if stateless_context_items.is_none() {
                    if let Some(item) = assistant_item {
                        stateless.push(item);
                    }
                    if include_tool_items {
                        stateless.extend(tool_call_items.clone());
                        stateless.extend(tool_outputs.clone());
                    }
                    stateless_context_items = Some(stateless.clone());
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
        stable_prefix_mode: bool,
        force_text: bool,
        current_input_items: &[Value],
        include_tool_items: bool,
        sub_agent_run_id: Option<String>,
    ) -> Vec<Value> {
        let mut items = Vec::new();
        let summary_count;
        let history_count;
        let mut summary_context_used = false;
        let mut tool_call_ids: HashSet<String> = HashSet::new();
        let mut tool_output_ids: HashSet<String> = HashSet::new();
        let context_data = if let Some(run_id) = sub_agent_run_id.as_ref() {
            self.message_manager
                .get_sub_agent_run_history_context(run_id, 2)
                .await
        } else if let Some(sid) = session_id.as_ref() {
            self.message_manager.get_chat_history_context(sid, 2).await
        } else {
            (None, 0, Vec::new())
        };

        let use_full_pending_history = stable_prefix_mode && history_limit >= self.history_limit;
        let (merged_summary, merged_summary_count, mut pending_history) = context_data;
        summary_count = merged_summary_count;
        if let Some(summary_text) = merged_summary {
            summary_context_used = true;
            items.push(to_message_item(
                "system",
                &Value::String(summary_text),
                force_text,
            ));
        }
        if !use_full_pending_history
            && history_limit > 0
            && pending_history.len() > history_limit as usize
        {
            let keep_from = pending_history.len() - history_limit as usize;
            pending_history = pending_history.split_off(keep_from);
        }
        let history = pending_history;

        history_count = history.len();

        if include_tool_items {
            for msg in &history {
                if msg.role == "tool" {
                    if let Some(call_id) = msg.tool_call_id.clone() {
                        if !call_id.is_empty() {
                            tool_output_ids.insert(call_id);
                        }
                    }
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
                continue;
            }

            if msg.role == "user"
                || msg.role == "assistant"
                || msg.role == "system"
                || msg.role == "developer"
            {
                items.push(to_message_item(
                    &msg.role,
                    &Value::String(msg.content.clone()),
                    force_text,
                ));
                if include_tool_items {
                    let mut tool_calls = msg.tool_calls.clone().or_else(|| {
                        msg.metadata
                            .as_ref()
                            .and_then(|m| m.get("toolCalls").cloned())
                    });
                    if let Some(Value::String(s)) = tool_calls.clone() {
                        if let Ok(v) = serde_json::from_str::<Value>(&s) {
                            tool_calls = Some(v);
                        }
                    }
                    if msg.role == "assistant" {
                        if let Some(arr) = tool_calls.and_then(|v| v.as_array().cloned()) {
                            for tc in arr {
                                let call_id = tc
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| tc.get("call_id").and_then(|v| v.as_str()))
                                    .unwrap_or("")
                                    .to_string();
                                if call_id.is_empty() {
                                    continue;
                                }
                                if !tool_output_ids.contains(&call_id) {
                                    continue;
                                }
                                let func = tc.get("function").cloned().unwrap_or(json!({}));
                                let name = func
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| tc.get("name").and_then(|v| v.as_str()))
                                    .unwrap_or("")
                                    .to_string();
                                let args = func
                                    .get("arguments")
                                    .cloned()
                                    .or_else(|| tc.get("arguments").cloned())
                                    .unwrap_or(Value::String("{}".to_string()));
                                let args_str = if let Some(s) = args.as_str() {
                                    s.to_string()
                                } else {
                                    args.to_string()
                                };
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

        if let Some(last) = items.last() {
            if last.get("type").and_then(|v| v.as_str()) == Some("message")
                && last.get("role").and_then(|v| v.as_str()) == Some("user")
            {
                items.pop();
            }
        }
        items.extend_from_slice(current_input_items);
        info!(
            "[AI_V3] stateless items built: stable_prefix_mode={}, summary_context_used={}, summaries={}, history_messages={}, total_items={}",
            stable_prefix_mode,
            summary_context_used,
            summary_count,
            history_count,
            items.len()
        );
        items
    }
}

fn usage_value_i64(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(|item| item.as_i64().or_else(|| item.as_u64().map(|v| v as i64)))
}

fn usage_nested_i64(value: &Value, parent: &str, key: &str) -> Option<i64> {
    value
        .get(parent)
        .and_then(|item| item.get(key))
        .and_then(|item| item.as_i64().or_else(|| item.as_u64().map(|v| v as i64)))
}

fn log_usage_snapshot(purpose: &str, usage: Option<&Value>) {
    let Some(usage) = usage else {
        return;
    };
    let input_tokens = usage_value_i64(usage, "input_tokens")
        .or_else(|| usage_value_i64(usage, "prompt_tokens"))
        .unwrap_or(-1);
    let output_tokens = usage_value_i64(usage, "output_tokens")
        .or_else(|| usage_value_i64(usage, "completion_tokens"))
        .unwrap_or(-1);
    let cached_tokens = usage_nested_i64(usage, "input_tokens_details", "cached_tokens")
        .or_else(|| usage_nested_i64(usage, "prompt_tokens_details", "cached_tokens"))
        .unwrap_or(0);

    info!(
        "[AI_V3] usage snapshot: purpose={}, input_tokens={}, cached_tokens={}, output_tokens={}",
        purpose, input_tokens, cached_tokens, output_tokens
    );
}

fn rewrite_system_messages_to_user(input: &Value, force_text_content: bool) -> Value {
    let Some(items) = input.as_array() else {
        return input.clone();
    };

    let mut changed = false;
    let mut mapped = Vec::with_capacity(items.len());

    for item in items {
        let item_type = item
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let role = item
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if item_type == "message" && (role == "system" || role == "developer") {
            let content = response_content_to_text(item.get("content").unwrap_or(&Value::Null));
            let label = if role == "developer" {
                "开发者上下文"
            } else {
                "系统上下文"
            };
            let wrapped = if content.trim().is_empty() {
                format!("【{}】", label)
            } else {
                format!("【{}】\n{}", label, content)
            };
            mapped.push(to_message_item(
                "user",
                &Value::String(wrapped),
                force_text_content,
            ));
            changed = true;
            continue;
        }

        mapped.push(item.clone());
    }

    if changed {
        Value::Array(mapped)
    } else {
        input.clone()
    }
}

fn response_content_to_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(array) = content.as_array() {
        let mut output = Vec::new();
        for part in array {
            if let Some(text) = part.as_str() {
                output.push(text.to_string());
                continue;
            }
            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                output.push(text.to_string());
                continue;
            }
            if let Some(text) = part.get("output_text").and_then(|value| value.as_str()) {
                output.push(text.to_string());
                continue;
            }
            output.push(part.to_string());
        }
        return output.join(
            "
",
        );
    }

    if let Some(object) = content.as_object() {
        if let Some(text) = object.get("text").and_then(|value| value.as_str()) {
            return text.to_string();
        }
        if let Some(text) = object.get("output").and_then(|value| value.as_str()) {
            return text.to_string();
        }
    }

    content.to_string()
}

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) {
            self.max_iterations = v;
        }
        if let Some(v) = effective.get("HISTORY_LIMIT").and_then(|v| v.as_i64()) {
            self.history_limit = v.max(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{response_content_to_text, rewrite_system_messages_to_user};

    #[test]
    fn rewrites_system_and_developer_messages_to_user_role() {
        let input = json!([
            {
                "type": "message",
                "role": "system",
                "content": [{"type":"input_text","text":"system prompt"}]
            },
            {
                "type": "message",
                "role": "developer",
                "content": [{"type":"input_text","text":"developer notes"}]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":"hello"}]
            }
        ]);

        let output = rewrite_system_messages_to_user(&input, false);
        let arr = output.as_array().expect("array output");
        assert_eq!(arr.len(), 3);
        assert_eq!(
            arr[0].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            arr[1].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            arr[2].get("role").and_then(|value| value.as_str()),
            Some("user")
        );

        let first_text = response_content_to_text(arr[0].get("content").unwrap_or(&Value::Null));
        let second_text = response_content_to_text(arr[1].get("content").unwrap_or(&Value::Null));
        assert!(first_text.contains("系统上下文"));
        assert!(first_text.contains("system prompt"));
        assert!(second_text.contains("开发者上下文"));
        assert!(second_text.contains("developer notes"));
    }

    #[test]
    fn keeps_input_unchanged_when_no_system_messages_exist() {
        let input = json!([
            {
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":"hello"}]
            }
        ]);

        let output = rewrite_system_messages_to_user(&input, false);
        assert_eq!(input, output);
    }
}
