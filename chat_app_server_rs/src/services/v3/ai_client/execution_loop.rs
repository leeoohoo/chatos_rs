use serde_json::Value;
use tracing::info;

use crate::core::tool_call::tool_calls_value_has_items;
use crate::modules::conversation_runtime::task_board::{
    build_hidden_task_turn_review_metadata, build_task_turn_follow_up_directive,
    build_task_turn_follow_up_message, build_task_turn_review_retry_guidance,
    parse_task_turn_review_outcome, strip_task_turn_review_marker, TaskTurnFollowUpMode,
    TaskTurnReviewOutcome,
};
use crate::services::ai_common::{
    build_ai_client_success_payload, completion_failed_error, execute_tool_lifecycle,
    handle_transient_retry, is_retryable_provider_overload_error, terminal_empty_response_error,
};
use crate::services::v3::ai_request_handler::StreamCallbacks;
use crate::utils::abort_registry;
use tokio::time::{sleep, Duration};
use tracing::warn;

use super::compat::{log_usage_snapshot, rewrite_system_messages_to_user};
use super::execution_loop_guidance::{load_runtime_guidance_input_items, prepend_input_items};
use super::execution_loop_tool_io::build_tool_output_items;
use super::prev_context::{
    base_url_disallows_system_messages, should_disable_prev_id_for_prefixed_input_items,
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
        prompt_cache_key: Option<String>,
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
        let mut remote_active_summary_attempted = false;
        let mut non_terminal_empty_retry_count = 0usize;
        let max_non_terminal_empty_retries = 3usize;
        let mut terminal_empty_retry_count = 0usize;
        let max_terminal_empty_retries = 2usize;
        let max_completion_retry_retries = 5usize;
        let mut completion_retry_count = 0usize;
        let max_task_follow_up_rounds = 3usize;
        let mut task_follow_up_rounds = 0usize;
        let mut task_follow_up_mode: Option<TaskTurnFollowUpMode> = None;
        let mut task_follow_up_locale: Option<
            crate::core::internal_context_locale::InternalContextLocale,
        > = None;
        let mut last_visible_completion_content: Option<String> = None;
        let mut last_visible_completion_reasoning: Option<String> = None;
        let mut last_visible_completion_finish_reason: Option<String> = None;

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

            let mut effective_prefixed_input_items = self
                .load_runtime_prefixed_input_items()
                .await
                .filter(|items| !items.is_empty())
                .unwrap_or_else(|| prefixed_input_items.clone());
            let runtime_guidance_items = load_runtime_guidance_input_items(
                session_id.as_deref(),
                turn_id.as_deref(),
                force_text_content,
                &callbacks,
            )
            .await;
            if !runtime_guidance_items.is_empty() {
                effective_prefixed_input_items.extend(runtime_guidance_items.clone());
            }

            self.maybe_refresh_stateless_context(
                session_id.as_deref(),
                stable_prefix_mode,
                use_prev_id,
                &raw_input,
                force_text_content,
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
                if let Some(cb) = &callbacks.on_before_model_request {
                    cb(
                        request_input.clone(),
                        if use_prev_id {
                            previous_response_id.clone()
                        } else {
                            None
                        },
                        None,
                    );
                }
                let stream_callbacks = if matches!(
                    task_follow_up_mode,
                    Some(TaskTurnFollowUpMode::ReviewExecution)
                ) {
                    StreamCallbacks {
                        on_chunk: None,
                        on_thinking: None,
                    }
                } else {
                    StreamCallbacks {
                        on_chunk: callbacks.on_chunk.clone(),
                        on_thinking: if reasoning_enabled {
                            callbacks.on_thinking.clone()
                        } else {
                            None
                        },
                    }
                };
                let request_metadata = if matches!(
                    task_follow_up_mode,
                    Some(TaskTurnFollowUpMode::ReviewExecution)
                ) {
                    Some(build_hidden_task_turn_review_metadata())
                } else {
                    None
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
                        prompt_cache_key.clone(),
                        if tools.is_empty() {
                            None
                        } else {
                            Some(tools.clone())
                        },
                        request_cwd.clone(),
                        Some(temperature),
                        max_tokens,
                        stream_callbacks,
                        Some(provider.clone()),
                        thinking_level.clone(),
                        session_id.clone(),
                        turn_id.clone(),
                        message_mode.clone(),
                        message_source.clone(),
                        request_metadata,
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
                                &mut previous_response_id,
                                &mut no_system_messages,
                                &mut remote_active_summary_attempted,
                                &mut stateless_context_items,
                                &mut input,
                                &callbacks,
                            )
                            .await
                        {
                            continue;
                        }
                        match handle_transient_retry(
                            "[AI_V3]",
                            err_msg.as_str(),
                            &mut transient_retry_count,
                            max_transient_retries,
                        )
                        .await
                        {
                            Ok(true) => continue,
                            Err(error_message) => {
                                last_error = Some(error_message);
                            }
                            Ok(false) => {}
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
                if is_retryable_provider_overload_error(err.as_str())
                    && completion_retry_count < max_completion_retry_retries
                {
                    completion_retry_count += 1;
                    let backoff_ms = 150_u64 * completion_retry_count as u64;
                    warn!(
                        "[AI_V3] completion failed with retryable provider overload; retry {}/{} after {}ms: {}",
                        completion_retry_count,
                        max_completion_retry_retries,
                        backoff_ms,
                        err
                    );
                    sleep(Duration::from_millis(backoff_ms)).await;
                    continue;
                }
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
                        &mut use_prev_id,
                        &mut can_use_prev_id,
                        &mut previous_response_id,
                        &mut remote_active_summary_attempted,
                        &mut stateless_context_items,
                        &mut input,
                        &callbacks,
                    )
                    .await
                {
                    continue;
                }
                if is_retryable_provider_overload_error(err.as_str()) {
                    return Err(format!(
                        "AI 请求失败：上游暂时过载，已重试 {} 次，最后错误：{}",
                        max_completion_retry_retries, err
                    ));
                }
                return Err(err);
            }

            completion_retry_count = 0;

            if matches!(
                task_follow_up_mode,
                Some(TaskTurnFollowUpMode::ReviewExecution)
            ) {
                let review_locale = task_follow_up_locale
                    .take()
                    .unwrap_or(crate::core::internal_context_locale::InternalContextLocale::ZhCn);
                match parse_task_turn_review_outcome(ai_response.content.as_str()) {
                    TaskTurnReviewOutcome::Pass => {
                        let final_content = last_visible_completion_content
                            .clone()
                            .unwrap_or_else(|| strip_task_turn_review_marker(ai_response.content.as_str()));
                        let final_reasoning = last_visible_completion_reasoning
                            .clone()
                            .or(ai_response.reasoning.clone());
                        let final_finish_reason = last_visible_completion_finish_reason
                            .clone()
                            .or(ai_response.finish_reason.clone());
                        return Ok(build_ai_client_success_payload(
                            final_content,
                            final_reasoning,
                            final_finish_reason,
                            iteration,
                        ));
                    }
                    TaskTurnReviewOutcome::NeedsMoreWork | TaskTurnReviewOutcome::Unknown => {
                        if task_follow_up_rounds < max_task_follow_up_rounds {
                            task_follow_up_rounds += 1;
                            if let Some(cb) = &callbacks.on_thinking {
                                cb("复查发现仍需处理，继续同一轮修正。".to_string());
                            }
                            let retry_input = build_task_turn_follow_up_message(
                                build_task_turn_review_retry_guidance(review_locale).as_str(),
                            );
                            input = retry_input;
                            previous_response_id = ai_response.response_id.clone();
                            use_prev_id = previous_response_id.is_some();
                            can_use_prev_id = can_use_prev_id && use_prev_id;
                            stateless_context_items = input.as_array().cloned();
                            task_follow_up_mode = Some(TaskTurnFollowUpMode::ContinueExecution);
                            iteration += 1;
                            continue;
                        }
                    }
                }
            }

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    return Err("aborted".to_string());
                }
            }

            let tool_calls = ai_response.tool_calls.clone();
            let has_tool_calls = tool_calls_value_has_items(tool_calls.as_ref());
            if !has_tool_calls {
                if self
                    .try_recover_from_non_terminal_empty_response(
                        &ai_response,
                        session_id.as_ref(),
                        turn_id.as_ref(),
                        &raw_input,
                        stable_prefix_mode,
                        include_tool_items,
                        effective_prefixed_input_items.as_slice(),
                        force_text_content,
                        &mut non_terminal_empty_retry_count,
                        max_non_terminal_empty_retries,
                        &mut use_prev_id,
                        &mut can_use_prev_id,
                        &mut previous_response_id,
                        &mut stateless_context_items,
                        &mut input,
                        &mut iteration,
                    )
                    .await?
                {
                    continue;
                }
                if self
                    .try_recover_from_terminal_empty_response(
                        &ai_response,
                        session_id.as_ref(),
                        turn_id.as_ref(),
                        &raw_input,
                        stable_prefix_mode,
                        include_tool_items,
                        effective_prefixed_input_items.as_slice(),
                        force_text_content,
                        &mut terminal_empty_retry_count,
                        max_terminal_empty_retries,
                        pending_tool_calls.as_ref(),
                        pending_tool_outputs.as_ref(),
                        &mut use_prev_id,
                        &mut can_use_prev_id,
                        &mut previous_response_id,
                        &mut stateless_context_items,
                        &mut input,
                        &mut iteration,
                    )
                    .await?
                {
                    continue;
                }
                if let Some(err) = terminal_empty_response_error(
                    ai_response.finish_reason.as_deref(),
                    ai_response.content.as_str(),
                    ai_response.reasoning.as_deref(),
                    ai_response.tool_calls.as_ref(),
                    ai_response.provider_error.as_ref(),
                ) {
                    return Err(err);
                }

                if let (Some(sid), Some(tid), Some(resp_id)) = (
                    session_id.as_deref(),
                    turn_id.as_deref(),
                    ai_response.response_id.as_deref(),
                ) {
                    if task_follow_up_rounds < max_task_follow_up_rounds {
                        if let Some(directive) = build_task_turn_follow_up_directive(sid, tid).await
                        {
                            last_visible_completion_content = Some(ai_response.content.clone());
                            last_visible_completion_reasoning = ai_response.reasoning.clone();
                            last_visible_completion_finish_reason =
                                ai_response.finish_reason.clone();
                            task_follow_up_rounds += 1;
                            task_follow_up_mode = Some(directive.mode);
                            task_follow_up_locale = Some(directive.locale);
                            if let Some(cb) = &callbacks.on_thinking {
                                cb(match directive.mode {
                                    TaskTurnFollowUpMode::ContinueExecution => {
                                        "检测到未完成任务，继续同一轮执行。".to_string()
                                    }
                                    TaskTurnFollowUpMode::ReviewExecution => {
                                        "任务看起来已完成，正在同一轮复查。".to_string()
                                    }
                                });
                            }
                            let follow_up_input =
                                build_task_turn_follow_up_message(directive.guidance.as_str());
                            input = follow_up_input;
                            previous_response_id = Some(resp_id.to_string());
                            use_prev_id = true;
                            can_use_prev_id = can_use_prev_id && use_prev_id;
                            stateless_context_items = input.as_array().cloned();
                            iteration += 1;
                            continue;
                        }
                    }
                }

                return Ok(build_ai_client_success_payload(
                    ai_response.content,
                    ai_response.reasoning,
                    ai_response.finish_reason,
                    iteration,
                ));
            }

            let raw_tool_calls = tool_calls.unwrap_or(Value::Array(vec![]));
            let tool_calls_arr = raw_tool_calls.as_array().cloned().unwrap_or_default();
            let execution_plan = build_tool_call_execution_plan(&tool_calls_arr);
            let display_tool_calls = Value::Array(execution_plan.display_calls.clone());
            let tool_call_items = build_tool_call_items(&tool_calls_arr);
            let mcp_tool_execute = self.mcp_tool_execute.clone();
            let message_manager = self.message_manager.clone();
            let persist_session_id = session_id.clone();
            let persisted_results = execute_tool_lifecycle(
                tool_calls_arr.as_slice(),
                display_tool_calls,
                session_id.as_deref(),
                persist_tool_messages,
                &callbacks,
                |on_tools_stream_cb| {
                    mcp_tool_execute.execute_tools_stream(
                        &execution_plan.execute_calls,
                        session_id.as_deref(),
                        turn_id.as_deref(),
                        Some(model.as_str()),
                        on_tools_stream_cb,
                    )
                },
                |results| expand_tool_results_with_aliases(results, &execution_plan.alias_map),
                move |results| {
                    let message_manager = message_manager.clone();
                    let persist_session_id = persist_session_id.clone();
                    async move {
                        if let Some(sid) = persist_session_id.as_ref() {
                            message_manager
                                .save_tool_results(sid, results.as_slice())
                                .await;
                        }
                    }
                },
            )
            .await?
            .persisted_results;

            let tool_outputs = build_tool_output_items(persisted_results.as_slice());
            let (next_input, next_prev_id, next_use_prev_id) = self
                .advance_after_tool_execution(
                    &ai_response,
                    session_id.as_ref(),
                    &raw_input,
                    stable_prefix_mode,
                    force_text_content,
                    effective_prefixed_input_items.as_slice(),
                    include_tool_items,
                    prefer_stateless,
                    use_prev_id,
                    &mut can_use_prev_id,
                    tool_call_items.as_slice(),
                    tool_outputs.as_slice(),
                    &mut stateless_context_items,
                    &mut pending_tool_calls,
                    &mut pending_tool_outputs,
                )
                .await;

            input = next_input;
            previous_response_id = next_prev_id;
            use_prev_id = next_use_prev_id;
            iteration += 1;
        }
    }
}
