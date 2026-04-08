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

const IM_PLANNING_MUTATION_REPLY_PROMPT_PREFIX: &str = "本轮任务规划或任务调整已经完成。不要再调用任何工具，不要继续轮询任务状态，也不要重复查询授权或运行时资产。";
const IM_PLANNING_MUTATION_REPLY_PROMPT_SUFFIX: &str =
    "请立即用简短自然语言向用户总结本轮结果，并结束当前回复。";
const IM_PLANNING_FINALIZE_FALLBACK_REPLY: &str =
    "本轮任务规划或任务调整已经处理完毕，后续将按新的任务状态异步推进。";

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
        message_mode: Option<String>,
        message_source: Option<String>,
        request_cwd: Option<String>,
    ) -> Result<Value, String> {
        let include_tool_items = !tools.is_empty();
        let persist_tool_messages = purpose != "agent_builder";
        let mut input = input;
        let mut tools = tools;
        let mut previous_response_id = previous_response_id;
        let mut use_prev_id = use_prev_id;
        let mut can_use_prev_id = can_use_prev_id;
        let mut force_text_content = force_text_content;
        let mut adaptive_history_limit = history_limit;
        let mut iteration = iteration;
        let mut pending_tool_outputs: Option<Vec<Value>> = None;
        let mut pending_tool_calls: Option<Vec<Value>> = None;
        let mut im_planning_finalize_only = false;
        let mut im_planning_finalize_fallback: Option<String> = None;
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
                                "session_id": applied_item.session_id,
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
            log_usage_snapshot(
                purpose,
                session_id.as_deref(),
                turn_id.as_deref(),
                iteration,
                use_prev_id,
                can_use_prev_id,
                ai_response.usage.as_ref(),
            );

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
                if im_planning_finalize_only {
                    let final_content = if has_content {
                        ai_response.content.clone()
                    } else {
                        im_planning_finalize_fallback
                            .clone()
                            .unwrap_or_else(|| IM_PLANNING_FINALIZE_FALLBACK_REPLY.to_string())
                    };
                    return Ok(json!({
                        "success": true,
                        "content": final_content,
                        "reasoning": ai_response.reasoning,
                        "tool_calls": Value::Null,
                        "finish_reason": ai_response.finish_reason,
                        "iteration": iteration
                    }));
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
            if im_planning_finalize_only {
                warn!(
                    "[AI_V3] ignore tool calls during IM planning finalize-only mode: session_id={}, turn_id={}",
                    session_id.as_deref().unwrap_or("n/a"),
                    turn_id.as_deref().unwrap_or("n/a")
                );
                let final_content = if ai_response.content.trim().is_empty() {
                    im_planning_finalize_fallback
                        .clone()
                        .unwrap_or_else(|| IM_PLANNING_FINALIZE_FALLBACK_REPLY.to_string())
                } else {
                    ai_response.content.clone()
                };
                return Ok(json!({
                    "success": true,
                    "content": final_content,
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

            if let Some((reply_prompt, fallback_reply)) = build_im_planning_finalize_reply(
                turn_id.as_deref(),
                purpose,
                expanded_tool_results.as_slice(),
            ) {
                warn!(
                    "[AI_V3] force IM planning turn to finish after successful mutation: session_id={}, turn_id={}",
                    session_id.as_deref().unwrap_or("n/a"),
                    turn_id.as_deref().unwrap_or("n/a")
                );
                runtime_guidance_items.push(build_im_planning_mutation_reply_input_item(
                    reply_prompt.as_str(),
                    force_text_content,
                ));
                if let (Some(sid), Some(tid)) = (session_id.as_deref(), turn_id.as_deref()) {
                    // Once planning mutation succeeds, this turn should only emit the final
                    // natural-language summary. New user messages must start a fresh IM run
                    // instead of being merged into this tool-free finalize phase.
                    runtime_guidance_manager().close_turn(sid, tid);
                }
                im_planning_finalize_only = true;
                im_planning_finalize_fallback = Some(fallback_reply);
                tools.clear();
                previous_response_id = None;
                use_prev_id = false;
                can_use_prev_id = false;
                iteration += 1;
                continue;
            }

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
        "user",
        &Value::String(format_runtime_guidance_instruction(guidance_item)),
        force_text_content,
    )
}

fn build_im_planning_mutation_reply_input_item(
    prompt: &str,
    force_text_content: bool,
) -> Value {
    to_message_item(
        "user",
        &Value::String(prompt.to_string()),
        force_text_content,
    )
}

fn format_runtime_guidance_instruction(guidance_item: &RuntimeGuidanceItem) -> String {
    guidance_item.content.trim().to_string()
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

fn build_im_planning_finalize_reply(
    turn_id: Option<&str>,
    purpose: &str,
    tool_results: &[ToolResult],
) -> Option<(String, String)> {
    if purpose != "chat" {
        return None;
    }
    if !turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.starts_with("im-run-"))
        .unwrap_or(false)
    {
        return None;
    }

    let summaries: Vec<String> = tool_results
        .iter()
        .filter_map(im_planning_mutation_summary)
        .collect();
    if summaries.is_empty() {
        return None;
    }

    let summary_text = summaries.join("；");
    Some((
        format!(
            "{} 已处理的任务变更：{}。{}",
            IM_PLANNING_MUTATION_REPLY_PROMPT_PREFIX,
            summary_text,
            IM_PLANNING_MUTATION_REPLY_PROMPT_SUFFIX
        ),
        format!("{} {}", summary_text, IM_PLANNING_FINALIZE_FALLBACK_REPLY),
    ))
}

fn is_successful_im_planning_mutation_result(result: &ToolResult) -> bool {
    im_planning_mutation_summary(result).is_some()
}

fn canonical_im_planning_tool_name(name: &str) -> Option<&'static str> {
    const PLANNING_MUTATION_TOOLS: [&str; 5] = [
        "create_tasks",
        "confirm_task",
        "request_pause_running_task",
        "request_stop_running_task",
        "resume_task",
    ];

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    PLANNING_MUTATION_TOOLS.iter().copied().find(|candidate| {
        trimmed == *candidate
            || trimmed
                .strip_suffix(candidate)
                .and_then(|prefix| prefix.strip_suffix('_'))
                .is_some()
    })
}

fn im_planning_mutation_summary(result: &ToolResult) -> Option<String> {
    if !result.success || result.is_error || result.is_stream {
        return None;
    }

    let payload = serde_json::from_str::<Value>(result.content.as_str()).ok();
    match canonical_im_planning_tool_name(result.name.as_str())? {
        "create_tasks" => {
            let confirmed = payload
                .as_ref()
                .and_then(|value| value.get("confirmed").and_then(Value::as_bool))
                == Some(true);
            let cancelled = payload
                .as_ref()
                .and_then(|value| value.get("cancelled").and_then(Value::as_bool))
                == Some(true);
            let created_count = payload
                .as_ref()
                .and_then(|value| value.get("created_count").and_then(Value::as_u64))
                .unwrap_or(0);
            if confirmed {
                let task_count = created_count.max(1);
                Some(format!("已创建 {} 个待确认任务", task_count))
            } else if cancelled {
                let reason = payload
                    .as_ref()
                    .and_then(|value| value.get("reason").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("user_cancelled");
                Some(match reason {
                    "review_timeout" => "任务创建确认已超时结束".to_string(),
                    _ => "任务创建已取消".to_string(),
                })
            } else {
                None
            }
        }
        "confirm_task" => payload
            .as_ref()
            .and_then(|value| value.get("task_id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|task_id| format!("已确认任务 {}", task_id)),
        "request_pause_running_task" | "request_stop_running_task" => payload
            .as_ref()
            .and_then(|value| value.get("requested").and_then(Value::as_bool))
            .filter(|requested| *requested)
            .map(|_| {
                if canonical_im_planning_tool_name(result.name.as_str())
                    == Some("request_pause_running_task")
                {
                    "已提交暂停当前任务的请求".to_string()
                } else {
                    "已提交停止当前任务的请求".to_string()
                }
            }),
        "resume_task" => payload
            .as_ref()
            .and_then(|value| value.get("task_id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|task_id| format!("已恢复任务 {}", task_id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_im_planning_finalize_reply, canonical_im_planning_tool_name,
        is_successful_im_planning_mutation_result,
    };
    use crate::core::mcp_tools::ToolResult;

    fn tool_result(name: &str, content: &str) -> ToolResult {
        ToolResult {
            tool_call_id: "call_1".to_string(),
            name: name.to_string(),
            success: true,
            is_error: false,
            is_stream: false,
            conversation_turn_id: Some("im-run-test".to_string()),
            content: content.to_string(),
        }
    }

    #[test]
    fn detects_successful_create_tasks_as_planning_mutation() {
        let result = tool_result(
            "create_tasks",
            r#"{"confirmed":true,"cancelled":false,"created_count":1}"#,
        );
        assert!(is_successful_im_planning_mutation_result(&result));
        assert!(build_im_planning_finalize_reply(
            Some("im-run-123"),
            "chat",
            &[result]
        )
        .is_some());
    }

    #[test]
    fn cancelled_create_tasks_also_finish_current_im_turn() {
        let result = tool_result(
            "create_tasks",
            r#"{"confirmed":false,"cancelled":true,"reason":"user_cancelled"}"#,
        );
        assert!(is_successful_im_planning_mutation_result(&result));
        assert!(build_im_planning_finalize_reply(
            Some("im-run-123"),
            "chat",
            &[result]
        )
        .is_some());
    }

    #[test]
    fn ignores_non_im_turns_even_when_mutation_succeeds() {
        let result = tool_result("confirm_task", r#"{"task_id":"task_1","status":"pending_execute"}"#);
        assert!(build_im_planning_finalize_reply(
            Some("task-exec-123"),
            "chat",
            &[result]
        )
        .is_none());
    }

    #[test]
    fn resume_task_is_treated_as_planning_mutation() {
        let result = tool_result("resume_task", r#"{"task_id":"task_2","status":"pending_execute"}"#);
        assert!(is_successful_im_planning_mutation_result(&result));
        assert!(build_im_planning_finalize_reply(
            Some("im-run-123"),
            "chat",
            &[result]
        )
        .is_some());
    }

    #[test]
    fn prefixed_builtin_tool_names_are_canonicalized() {
        assert_eq!(
            canonical_im_planning_tool_name("contact_task_create_tasks"),
            Some("create_tasks")
        );
        assert_eq!(
            canonical_im_planning_tool_name("builtin_confirm_task"),
            Some("confirm_task")
        );
        assert_eq!(
            canonical_im_planning_tool_name("task_executor_request_pause_running_task"),
            Some("request_pause_running_task")
        );
        assert_eq!(
            canonical_im_planning_tool_name("task_executor_request_stop_running_task"),
            Some("request_stop_running_task")
        );
        assert_eq!(
            canonical_im_planning_tool_name("task_runtime_resume_task"),
            Some("resume_task")
        );
    }

    #[test]
    fn prefixed_create_tasks_also_finish_current_im_turn() {
        let result = tool_result(
            "contact_task_create_tasks",
            r#"{"confirmed":true,"cancelled":false,"created_count":2}"#,
        );
        assert!(is_successful_im_planning_mutation_result(&result));
        assert!(build_im_planning_finalize_reply(
            Some("im-run-123"),
            "chat",
            &[result]
        )
        .is_some());
    }
}
