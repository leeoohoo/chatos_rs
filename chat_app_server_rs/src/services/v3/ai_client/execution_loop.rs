use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::core::mcp_tools::ToolResult;
use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::runtime_guidance_manager::{
    runtime_guidance_manager, RuntimeGuidanceItem, DEFAULT_DRAIN_LIMIT,
};
use crate::services::v3::ai_request_handler::StreamCallbacks;
use crate::utils::abort_registry;

use super::compat::{
    cap_tool_output_for_input, log_usage_snapshot, rewrite_system_messages_to_user,
};
use super::input_transform::{build_current_input_items, to_message_item};
use super::prev_context::{
    base_url_disallows_system_messages, is_response_parse_error,
    is_transient_transport_or_parse_error, should_disable_prev_id_for_prefixed_input_items,
    should_use_prev_id_for_next_turn,
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
        prefixed_input_items: Vec<Value>,
        prefer_stateless: bool,
        _allow_tool_image_input: bool,
        _use_codex_gateway_mcp_passthrough: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
        request_cwd: Option<String>,
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
        if use_prev_id
            && should_disable_prev_id_for_prefixed_input_items(prefixed_input_items.as_slice())
        {
            info!(
                "[AI_V3] disable previous_response_id inside execution loop because runtime prefixed input items are present: session_id={}",
                session_id.clone().unwrap_or_else(|| "n/a".to_string())
            );
            use_prev_id = false;
            can_use_prev_id = false;
            previous_response_id = None;
        }
        let mut stateless_context_items = if !use_prev_id {
            input.as_array().cloned()
        } else {
            None
        };
        let mut runtime_guidance_items: Vec<Value> = Vec::new();
        let mut non_terminal_empty_retry_count = 0usize;
        let max_non_terminal_empty_retries = 3usize;

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

            let mut effective_prefixed_input_items = prefixed_input_items.clone();
            if let (Some(sid), Some(tid)) = (session_id.as_deref(), turn_id.as_deref()) {
                let drained_guidance =
                    runtime_guidance_manager().drain_guidance(sid, tid, DEFAULT_DRAIN_LIMIT);
                for guidance_item in drained_guidance {
                    runtime_guidance_items.push(build_runtime_guidance_input_item(
                        &guidance_item,
                        force_text_content,
                    ));
                    if let Some(applied_item) = runtime_guidance_manager().mark_applied(
                        sid,
                        tid,
                        &guidance_item.guidance_id,
                    ) {
                        if let Some(cb) = &callbacks.on_runtime_guidance_applied {
                            cb(json!({
                                "guidance_id": applied_item.guidance_id,
                                "conversation_id": applied_item.session_id,
                                "turn_id": applied_item.turn_id,
                                "status": "applied",
                                "created_at": applied_item.created_at,
                                "applied_at": applied_item.applied_at,
                                "pending_count": runtime_guidance_manager().pending_count(sid, tid),
                            }));
                        }
                    }
                }
            }
            if !runtime_guidance_items.is_empty() {
                effective_prefixed_input_items.extend(runtime_guidance_items.clone());
            }

            self.maybe_refresh_stateless_context(
                session_id.as_deref(),
                stable_prefix_mode,
                use_prev_id,
                &raw_input,
                force_text_content,
                adaptive_history_limit,
                include_tool_items,
                effective_prefixed_input_items.as_slice(),
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
                let request_input_source = if use_prev_id && !runtime_guidance_items.is_empty() {
                    prepend_input_items(
                        &input,
                        runtime_guidance_items.as_slice(),
                        force_text_content,
                    )
                } else {
                    input.clone()
                };
                let request_input = if no_system_messages {
                    rewrite_system_messages_to_user(&request_input_source, force_text_content)
                } else {
                    request_input_source
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
                        request_cwd.clone(),
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
                                effective_prefixed_input_items.as_slice(),
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
                        effective_prefixed_input_items.as_slice(),
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
            let has_tool_calls = tool_calls
                .as_ref()
                .and_then(|v| v.as_array())
                .map(|a| a.is_empty())
                .map(|is_empty| !is_empty)
                .unwrap_or(false);
            if !has_tool_calls {
                let finish_reason = ai_response.finish_reason.as_deref();
                let has_content = !ai_response.content.trim().is_empty();
                let has_reasoning = ai_response
                    .reasoning
                    .as_deref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false);
                if is_non_terminal_finish_reason(finish_reason) && !has_content && !has_reasoning {
                    non_terminal_empty_retry_count += 1;
                    let response_id_for_log = ai_response
                        .response_id
                        .as_deref()
                        .unwrap_or("none")
                        .to_string();
                    warn!(
                        "[AI_V3] non-terminal empty response detected: session_id={}, turn_id={}, finish_reason={}, response_id={}, iteration={}, retry={}/{}",
                        session_id.as_deref().unwrap_or("n/a"),
                        turn_id.as_deref().unwrap_or("n/a"),
                        finish_reason.unwrap_or("none"),
                        response_id_for_log,
                        iteration,
                        non_terminal_empty_retry_count,
                        max_non_terminal_empty_retries,
                    );

                    if non_terminal_empty_retry_count > max_non_terminal_empty_retries {
                        return Err(format!(
                            "AI 响应未完成（finish_reason={}）且未返回内容，重试 {} 次后仍未恢复",
                            finish_reason.unwrap_or("unknown"),
                            max_non_terminal_empty_retries
                        ));
                    }

                    if use_prev_id {
                        warn!(
                            "[AI_V3] disable previous_response_id after non-terminal empty response: session_id={}",
                            session_id.as_deref().unwrap_or("n/a")
                        );
                        if let Some(sid) = session_id.as_ref() {
                            self.prev_response_id_disabled_sessions.insert(sid.clone());
                        }
                        can_use_prev_id = false;
                        use_prev_id = false;
                        previous_response_id = None;
                        let stateless = if let Some(items) = stateless_context_items.clone() {
                            items
                        } else {
                            self.build_stateless_from_raw_input(
                                session_id.as_ref(),
                                &raw_input,
                                force_text_content,
                                adaptive_history_limit,
                                stable_prefix_mode,
                                include_tool_items,
                                effective_prefixed_input_items.as_slice(),
                            )
                            .await
                        };
                        if !stateless.is_empty() {
                            stateless_context_items = Some(stateless.clone());
                            input = Value::Array(stateless);
                        }
                    }

                    let backoff_ms = 200_u64 * non_terminal_empty_retry_count as u64;
                    sleep(Duration::from_millis(backoff_ms)).await;
                    iteration += 1;
                    continue;
                }
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
                cb(json!({
                    "tool_results": tool_results.clone(),
                }));
            }

            if persist_tool_messages {
                if let Some(sid) = session_id.as_ref() {
                    self.message_manager
                        .save_tool_results(sid, expanded_tool_results.as_slice())
                        .await;
                }
            }

            let tool_outputs = build_tool_output_items(expanded_tool_results.as_slice());
            let turn_tool_input_items = tool_outputs.clone();
            pending_tool_outputs = Some(turn_tool_input_items.clone());
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

            let mut next_input = Value::Array(turn_tool_input_items.clone());
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
                        effective_prefixed_input_items.as_slice(),
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

fn build_runtime_guidance_input_item(
    guidance_item: &RuntimeGuidanceItem,
    force_text_content: bool,
) -> Value {
    to_message_item(
        "system",
        &Value::String(format_runtime_guidance_instruction(guidance_item)),
        force_text_content,
    )
}

fn format_runtime_guidance_instruction(guidance_item: &RuntimeGuidanceItem) -> String {
    format!(
        "[Runtime Guidance]\n- guidance_id: {}\n- time: {}\n- source: user guidance during running turn\n- instruction: {}\n- rule: treat this as high-priority preference unless conflicts with safety",
        guidance_item.guidance_id,
        guidance_item.created_at,
        guidance_item.content
    )
}

fn prepend_input_items(input: &Value, prefixed_items: &[Value], force_text_content: bool) -> Value {
    if prefixed_items.is_empty() {
        return input.clone();
    }
    let mut merged = prefixed_items.to_vec();
    merged.extend(build_current_input_items(input, force_text_content));
    Value::Array(merged)
}

fn is_non_terminal_finish_reason(finish_reason: Option<&str>) -> bool {
    let normalized = finish_reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    matches!(
        normalized.as_deref(),
        Some("in_progress") | Some("queued") | Some("pending") | Some("incomplete")
    )
}

fn build_tool_output_items(tool_results: &[ToolResult]) -> Vec<Value> {
    tool_results
        .iter()
        .map(|result| {
            let output_text = cap_tool_output_for_input(result.content.as_str());
            json!({
                "type": "function_call_output",
                "call_id": result.tool_call_id,
                "output": output_text
            })
        })
        .collect()
}
