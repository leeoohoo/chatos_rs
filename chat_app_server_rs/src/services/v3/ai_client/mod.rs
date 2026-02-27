use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{json, Value};
use tracing::info;
use tracing::warn;

use crate::config::Config;
use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::summary::engine::{
    maybe_summarize as maybe_summarize_with_engine,
    retry_after_context_overflow as retry_after_context_overflow_with_engine,
};
use crate::services::summary::persist::persist_summary;
use crate::services::summary::types::{
    build_summarizer_system_prompt, PersistSummaryPayload, SummaryCallbacks, SummaryOptions,
    SummarySourceInfo, SummaryTrigger,
};
use crate::services::user_settings::AiClientSettings;
use crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;
use crate::services::v3::summary_adapter::V3SummaryAdapter;
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
    is_invalid_input_text_error, is_missing_tool_call_error, is_system_messages_not_allowed_error,
    is_unsupported_previous_response_id_error, reduce_history_limit,
    should_use_prev_id_for_next_turn,
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
    merge_target_summary_tokens: Option<i64>,
    dynamic_summary_enabled: bool,
    summary_bisect_enabled: bool,
    summary_bisect_max_depth: i64,
    summary_bisect_min_messages: i64,
    summary_retry_on_context_overflow: bool,
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
        let cfg = Config::get();
        Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            history_limit: 20,
            system_prompt: None,
            summary_threshold: cfg.summary_message_limit,
            summary_keep_last_n: cfg.summary_keep_last_n,
            max_context_tokens: Some(cfg.summary_max_context_tokens),
            target_summary_tokens: Some(cfg.summary_target_tokens),
            merge_target_summary_tokens: Some(cfg.summary_merge_target_tokens),
            dynamic_summary_enabled: cfg.dynamic_summary_enabled,
            summary_bisect_enabled: cfg.summary_bisect_enabled,
            summary_bisect_max_depth: cfg.summary_bisect_max_depth,
            summary_bisect_min_messages: cfg.summary_bisect_min_messages,
            summary_retry_on_context_overflow: cfg.summary_retry_on_context_overflow,
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

        let prefer_stateless = history_limit != 0;
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
            "[AI_V3] context mode: use_prev_id={}, can_use_prev_id={}, provider={}, history_limit={}, has_prev_id={}",
            use_prev_id,
            can_use_prev_id,
            provider,
            stateless_history_limit,
            previous_response_id.is_some()
        );
        let initial_input = if use_prev_id {
            normalize_input_for_provider(&raw_input, force_text_content)
        } else {
            let current_items = build_current_input_items(&raw_input, force_text_content);
            Value::Array(
                self.build_stateless_items(
                    session_id.clone(),
                    stateless_history_limit,
                    force_text_content,
                    &current_items,
                    include_tool_items,
                )
                .await,
            )
        };

        self.process_with_tools(
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
            force_text_content,
            prefer_stateless,
        )
        .await
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
        force_text_content: bool,
        prefer_stateless: bool,
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

            if !use_prev_id {
                if let Some(compacted) = self
                    .maybe_proactive_summarize_stateless_input(
                        &input,
                        &model,
                        session_id.clone(),
                        &callbacks,
                        force_text_content,
                    )
                    .await?
                {
                    stateless_context_items = compacted.as_array().cloned();
                    input = compacted;
                }
            }

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
                        callbacks.on_chunk.is_some() || callbacks.on_thinking.is_some(),
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
                                    force_text_content,
                                    &current_items,
                                    include_tool_items,
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
                                    force_text_content,
                                    &current_items,
                                    include_tool_items,
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
                        if is_context_length_exceeded_error(&err_msg) {
                            if let Some(compacted) = self
                                .try_summary_after_context_overflow(
                                    &input,
                                    &err_msg,
                                    &model,
                                    session_id.clone(),
                                    &callbacks,
                                    force_text_content,
                                )
                                .await?
                            {
                                can_use_prev_id = false;
                                use_prev_id = false;
                                previous_response_id = None;
                                stateless_context_items = compacted.as_array().cloned();
                                input = compacted;
                                continue;
                            }
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
                                        force_text_content,
                                        &current_items,
                                        include_tool_items,
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

            if let Some(err) = completion_failed_error(
                ai_response.finish_reason.as_deref(),
                ai_response.content.as_str(),
                ai_response.reasoning.as_deref(),
                ai_response.provider_error.as_ref(),
            ) {
                if is_context_length_exceeded_error(&err) {
                    if let Some(compacted) = self
                        .try_summary_after_context_overflow(
                            &input,
                            &err,
                            &model,
                            session_id.clone(),
                            &callbacks,
                            force_text_content,
                        )
                        .await?
                    {
                        can_use_prev_id = false;
                        use_prev_id = false;
                        previous_response_id = None;
                        stateless_context_items = compacted.as_array().cloned();
                        input = compacted;
                        continue;
                    }
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
                                force_text_content,
                                &current_items,
                                include_tool_items,
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
                        force_text_content,
                        &current_items,
                        include_tool_items,
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

    fn summary_options_for_model(&self, model: &str) -> SummaryOptions {
        let max_context_tokens = self.max_context_tokens.unwrap_or(6000);
        let target_summary_tokens = self.target_summary_tokens.unwrap_or(700);
        let merge_target_tokens = self
            .merge_target_summary_tokens
            .unwrap_or(target_summary_tokens);

        SummaryOptions {
            message_limit: self.summary_threshold,
            max_context_tokens,
            keep_last_n: self.summary_keep_last_n.max(0) as usize,
            target_summary_tokens,
            merge_target_tokens,
            model: model.to_string(),
            temperature: 0.2,
            bisect_enabled: self.summary_bisect_enabled,
            bisect_max_depth: self.summary_bisect_max_depth.max(1) as usize,
            bisect_min_messages: self.summary_bisect_min_messages.max(1) as usize,
            retry_on_context_overflow: self.summary_retry_on_context_overflow,
        }
    }

    fn build_summary_callbacks(callbacks: &AiClientCallbacks) -> Option<SummaryCallbacks> {
        let on_stream = callbacks.on_context_summarized_stream.clone().map(|cb| {
            Arc::new(move |chunk: String| {
                cb(Value::String(chunk));
            }) as Arc<dyn Fn(String) + Send + Sync>
        });

        let mapped = SummaryCallbacks {
            on_start: callbacks.on_context_summarized_start.clone(),
            on_stream,
            on_end: callbacks.on_context_summarized_end.clone(),
        };

        if mapped.on_start.is_none() && mapped.on_stream.is_none() && mapped.on_end.is_none() {
            None
        } else {
            Some(mapped)
        }
    }

    async fn maybe_proactive_summarize_stateless_input(
        &mut self,
        input: &Value,
        model: &str,
        session_id: Option<String>,
        callbacks: &AiClientCallbacks,
        force_text_content: bool,
    ) -> Result<Option<Value>, String> {
        if !self.dynamic_summary_enabled {
            return Ok(None);
        }

        let options = self.summary_options_for_model(model);
        let messages = response_input_to_chat_messages(input);
        if messages.is_empty() {
            return Ok(None);
        }

        let adapter = V3SummaryAdapter::new(
            self.ai_request_handler.clone(),
            self.message_manager.clone(),
        );
        let result = maybe_summarize_with_engine(
            &adapter,
            messages.as_slice(),
            &options,
            session_id.clone(),
            Self::build_summary_callbacks(callbacks),
            SummaryTrigger::Proactive,
        )
        .await?;

        if !result.summarized {
            return Ok(None);
        }

        self.persist_summary_for_session(
            session_id.as_deref(),
            &result,
            &options,
            SummaryTrigger::Proactive,
        )
        .await;

        Ok(Some(build_input_from_summary_result(
            &result,
            force_text_content,
        )))
    }

    async fn try_summary_after_context_overflow(
        &mut self,
        input: &Value,
        err: &str,
        model: &str,
        session_id: Option<String>,
        callbacks: &AiClientCallbacks,
        force_text_content: bool,
    ) -> Result<Option<Value>, String> {
        if !self.dynamic_summary_enabled {
            return Ok(None);
        }

        let options = self.summary_options_for_model(model);
        let messages = response_input_to_chat_messages(input);
        if messages.is_empty() {
            return Ok(None);
        }

        let adapter = V3SummaryAdapter::new(
            self.ai_request_handler.clone(),
            self.message_manager.clone(),
        );
        let result = retry_after_context_overflow_with_engine(
            &adapter,
            messages.as_slice(),
            err,
            &options,
            session_id.clone(),
            Self::build_summary_callbacks(callbacks),
        )
        .await?;

        let Some(result) = result else {
            return Ok(None);
        };

        self.persist_summary_for_session(
            session_id.as_deref(),
            &result,
            &options,
            SummaryTrigger::OverflowRetry,
        )
        .await;

        Ok(Some(build_input_from_summary_result(
            &result,
            force_text_content,
        )))
    }

    async fn persist_summary_for_session(
        &self,
        session_id: Option<&str>,
        result: &crate::services::summary::types::SummaryResult,
        options: &SummaryOptions,
        trigger: SummaryTrigger,
    ) {
        let Some(sid) = session_id else {
            return;
        };
        let Some(summary_text) = result.summary_text.as_ref() else {
            return;
        };

        let records: Vec<_> = self
            .message_manager
            .get_session_messages(sid, None)
            .await
            .into_iter()
            .filter(|message| {
                message
                    .metadata
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(|value| value.as_str())
                    != Some("session_summary")
            })
            .collect();

        let source = build_source_info(
            records.as_slice(),
            result.summarized_messages.len(),
            result.kept_messages.len(),
        );

        let adapter = V3SummaryAdapter::new(
            self.ai_request_handler.clone(),
            self.message_manager.clone(),
        );
        let payload = PersistSummaryPayload {
            session_id: sid.to_string(),
            summary_text: summary_text.clone(),
            summary_prompt: build_summarizer_system_prompt(options.target_summary_tokens),
            model: options.model.clone(),
            temperature: options.temperature,
            target_summary_tokens: options.target_summary_tokens,
            keep_last_n: options.keep_last_n as i64,
            approx_tokens: result.stats.input_tokens,
            trigger,
            truncated: result.truncated,
            stats: result.stats.clone(),
            source,
        };

        match persist_summary(&adapter, payload).await {
            Ok(outcome) => {
                if let Some(summary_id) = outcome.summary_id {
                    info!("[AI_V3] persisted summary_id={}", summary_id);
                }
            }
            Err(err) => {
                warn!("[AI_V3] persist summary failed: {}", err);
            }
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
                let limit = if history_limit > 0 {
                    Some(history_limit)
                } else {
                    None
                };
                let summary_limit = Some(2);
                let (summaries, history) = self
                    .message_manager
                    .get_session_history_with_summaries(sid, limit, summary_limit)
                    .await;
                let has_summary_table = !summaries.is_empty();
                summary_count = summaries.len();
                history_count = history.len();
                if has_summary_table {
                    for summary in summaries {
                        if !summary.summary_text.is_empty() {
                            let content = format!(
                                "以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}",
                                summary.summary_text
                            );
                            items.push(to_message_item(
                                "system",
                                &Value::String(content),
                                force_text,
                            ));
                        }
                    }
                }
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
                        if has_summary_table {
                            continue;
                        }
                        if let Some(summary) = msg.summary.clone() {
                            let content = format!(
                                "以下是之前对话与工具调用的摘要（可视为“压缩记忆”）：\n\n{}",
                                summary
                            );
                            items.push(to_message_item(
                                "system",
                                &Value::String(content),
                                force_text,
                            ));
                        }
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

fn build_source_info(
    records: &[crate::models::message::Message],
    summarized_messages_len: usize,
    kept_messages_len: usize,
) -> SummarySourceInfo {
    if records.is_empty() || summarized_messages_len == 0 {
        return SummarySourceInfo::default();
    }

    let total = records.len();
    let kept_start = total.saturating_sub(kept_messages_len);
    let summarize_end = kept_start.min(total);
    let summarize_start = summarize_end.saturating_sub(summarized_messages_len.min(summarize_end));

    if summarize_start >= summarize_end {
        return SummarySourceInfo::default();
    }

    let slice = &records[summarize_start..summarize_end];
    SummarySourceInfo {
        message_ids: slice.iter().map(|item| item.id.clone()).collect(),
        first_message_id: slice.first().map(|item| item.id.clone()),
        last_message_id: slice.last().map(|item| item.id.clone()),
        first_message_created_at: slice.first().map(|item| item.created_at.clone()),
        last_message_created_at: slice.last().map(|item| item.created_at.clone()),
    }
}

fn response_input_to_chat_messages(input: &Value) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(items) = input.as_array() {
        for item in items {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            match item_type {
                "message" => {
                    let role = item
                        .get("role")
                        .and_then(|value| value.as_str())
                        .unwrap_or("user");
                    let content = item.get("content").cloned().unwrap_or(Value::Null);
                    let content_text = response_content_to_text(&content);
                    messages.push(json!({"role": role, "content": content_text}));
                }
                "function_call" => {
                    let call_id = item
                        .get("call_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = item
                        .get("arguments")
                        .cloned()
                        .unwrap_or(Value::String("{}".to_string()));
                    messages.push(json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": call_id,
                            "function": {
                                "name": name,
                                "arguments": if let Some(raw) = arguments.as_str() { Value::String(raw.to_string()) } else { arguments }
                            }
                        }]
                    }));
                }
                "function_call_output" => {
                    let call_id = item
                        .get("call_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    let output = item.get("output").cloned().unwrap_or(Value::Null);
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": response_content_to_text(&output)
                    }));
                }
                _ => {
                    messages.push(json!({
                        "role": "user",
                        "content": item.to_string()
                    }));
                }
            }
        }
    }

    messages
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

fn build_input_from_summary_result(
    result: &crate::services::summary::types::SummaryResult,
    force_text_content: bool,
) -> Value {
    let mut items = Vec::new();

    if let Some(summary_prompt) = result.system_prompt.as_ref() {
        items.push(to_message_item(
            "system",
            &Value::String(summary_prompt.clone()),
            force_text_content,
        ));
    }

    for message in result.kept_messages.as_slice() {
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("user");
        if role == "tool" {
            let call_id = message
                .get("tool_call_id")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let content = response_content_to_text(message.get("content").unwrap_or(&Value::Null));
            let wrapped = if call_id.is_empty() {
                format!(
                    "[tool output]
{}",
                    content
                )
            } else {
                format!(
                    "[tool:{}]
{}",
                    call_id, content
                )
            };
            items.push(to_message_item(
                "assistant",
                &Value::String(wrapped),
                force_text_content,
            ));
            continue;
        }

        let content = response_content_to_text(message.get("content").unwrap_or(&Value::Null));
        items.push(to_message_item(
            role,
            &Value::String(content),
            force_text_content,
        ));
    }

    Value::Array(items)
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
            self.max_context_tokens = Some(v);
        }
        if let Some(v) = effective
            .get("SUMMARY_TARGET_TOKENS")
            .and_then(|v| v.as_i64())
        {
            self.target_summary_tokens = Some(v);
        }
        if let Some(v) = effective
            .get("SUMMARY_MERGE_TARGET_TOKENS")
            .and_then(|v| v.as_i64())
        {
            self.merge_target_summary_tokens = Some(v);
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
        if let Some(v) = effective
            .get("DYNAMIC_SUMMARY_ENABLED")
            .and_then(|v| v.as_bool())
        {
            self.dynamic_summary_enabled = v;
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
