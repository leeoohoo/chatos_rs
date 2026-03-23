use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::v3::ai_request_handler::StreamCallbacks;
use crate::utils::abort_registry;

use super::compat::{
    cap_tool_output_for_input, log_usage_snapshot, rewrite_system_messages_to_user,
};
use super::input_transform::{build_current_input_items, to_message_item};
use super::prev_context::{
    base_url_disallows_system_messages, is_response_parse_error,
    is_transient_transport_or_parse_error, should_use_prev_id_for_next_turn,
};
use super::tool_plan::{
    build_tool_call_execution_plan, build_tool_call_items, expand_tool_results_with_aliases,
};
use super::{AiClient, AiClientCallbacks};

impl AiClient {
    pub(super) async fn process_with_tools(
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
    ) -> Result<Value, String> {
        let include_tool_items = !tools.is_empty();
        let persist_tool_messages = purpose != "agent_builder";
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

            self.maybe_refresh_stateless_context(
                session_id.as_deref(),
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
            let max_transient_retries = 5usize;
            let mut transient_retry_count = 0usize;
            let mut request_attempt_guard = 0usize;
            let max_request_attempts = max_transient_retries + 12;

            loop {
                request_attempt_guard += 1;
                if request_attempt_guard > max_request_attempts {
                    break;
                }
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
                        if self
                            .try_recover_from_request_error(
                                err_msg.as_str(),
                                session_id.as_ref(),
                                &raw_input,
                                stable_prefix_mode,
                                include_tool_items,
                                pending_tool_calls.as_ref(),
                                pending_tool_outputs.as_ref(),
                                &mut use_prev_id,
                                &mut can_use_prev_id,
                                &mut force_text_content,
                                &mut adaptive_history_limit,
                                &mut previous_response_id,
                                &mut no_system_messages,
                                &mut stateless_context_items,
                                &mut input,
                            )
                            .await
                        {
                            continue;
                        }
                        if is_transient_transport_or_parse_error(err_msg.as_str()) {
                            let retry_kind = if is_response_parse_error(err_msg.as_str()) {
                                "响应解析异常"
                            } else {
                                "网络波动"
                            };
                            if transient_retry_count < max_transient_retries {
                                transient_retry_count += 1;
                                let backoff_ms = 150_u64 * transient_retry_count as u64;
                                warn!(
                                    "[AI_V3] transient {} detected; retry {}/{} after {}ms: {}",
                                    retry_kind,
                                    transient_retry_count,
                                    max_transient_retries,
                                    backoff_ms,
                                    err_msg
                                );
                                sleep(Duration::from_millis(backoff_ms)).await;
                                continue;
                            }
                            last_error = Some(format!(
                                "AI 请求失败：{}，已重试 {} 次，最后错误：{}",
                                retry_kind, max_transient_retries, err_msg
                            ));
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
                if self
                    .try_recover_from_completion_error(
                        err.as_str(),
                        session_id.as_ref(),
                        &raw_input,
                        stable_prefix_mode,
                        include_tool_items,
                        pending_tool_calls.as_ref(),
                        pending_tool_outputs.as_ref(),
                        force_text_content,
                        &mut adaptive_history_limit,
                        &mut use_prev_id,
                        &mut can_use_prev_id,
                        &mut previous_response_id,
                        &mut stateless_context_items,
                        &mut input,
                    )
                    .await
                {
                    continue;
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
                        "output": cap_tool_output_for_input(r.content.as_str())
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
}
