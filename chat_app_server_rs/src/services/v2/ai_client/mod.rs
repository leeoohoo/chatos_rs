use serde_json::{json, Value};
use tracing::info;

use crate::config::Config;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::messages::build_assistant_message_with_parts;
use crate::core::tool_call::build_tool_role_message;
use crate::models::session::Session;
use crate::modules::conversation_runtime::guidance::{
    build_runtime_guidance_applied_event, drain_runtime_guidance_items,
    format_runtime_guidance_instruction, resolve_runtime_guidance_locale, RuntimeGuidanceItem,
};
pub use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::{
    build_ai_client_success_payload, completion_failed_error, execute_tool_lifecycle,
    handle_transient_retry,
};
use crate::services::chatos_memory_engine;
use crate::modules::conversation_runtime::task_board::{
    build_hidden_task_turn_review_metadata, build_task_turn_follow_up_directive,
    build_task_turn_follow_up_message, build_task_turn_review_retry_guidance,
    parse_task_turn_review_outcome, strip_task_turn_review_marker, TaskTurnFollowUpMode,
    TaskTurnReviewOutcome,
};
use crate::services::task_board_refresh_context::TaskBoardRefreshContextStore;
use crate::services::user_settings::AiClientSettings;
use crate::services::v2::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v2::mcp_tool_execute::McpToolExecute;
use crate::services::v2::message_manager::MessageManager;
use crate::utils::abort_registry;

mod context_memory;
mod history_tools;
mod runtime_support;
mod token_compaction;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

use self::history_tools::{
    drop_duplicate_tail, ensure_tool_responses, sanitize_messages_for_request,
};
use self::runtime_support::cap_tool_content_for_input;
use self::token_compaction::is_token_limit_error;

pub struct AiClient {
    ai_request_handler: AiRequestHandler,
    mcp_tool_execute: McpToolExecute,
    message_manager: MessageManager,
    max_iterations: i64,
    system_prompt: Option<String>,
    task_board_refresh_context: TaskBoardRefreshContextStore,
}

const MAX_TASK_FOLLOW_UP_ROUNDS: usize = 3;
const TASK_FOLLOW_UP_ROLE_METADATA_KEY: &str = "task_follow_up";

impl AiClient {
    pub fn new(
        ai_request_handler: AiRequestHandler,
        mcp_tool_execute: McpToolExecute,
        message_manager: MessageManager,
    ) -> Result<Self, String> {
        let _cfg = Config::try_get()?;
        Ok(Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            system_prompt: None,
            task_board_refresh_context: TaskBoardRefreshContextStore::new(),
        })
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
    }

    pub fn set_mcp_tool_execute(&mut self, mcp_tool_execute: McpToolExecute) {
        self.mcp_tool_execute = mcp_tool_execute;
    }

    pub fn set_task_board_refresh_context(
        &mut self,
        session_id: Option<String>,
        turn_id: Option<String>,
        locale: InternalContextLocale,
        contact_system_prompt: Option<String>,
        builtin_mcp_system_prompt: Option<String>,
        command_system_prompt: Option<String>,
    ) {
        self.task_board_refresh_context.set(
            session_id,
            turn_id,
            locale,
            contact_system_prompt,
            builtin_mcp_system_prompt,
            command_system_prompt,
        );
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
        prefixed_messages: Vec<Value>,
    ) -> Result<Value, String> {
        let resolved_purpose = purpose.unwrap_or_else(|| "chat".to_string());
        let mut all_messages: Vec<Value> = Vec::new();

        if let Some(prompt) = self.system_prompt.clone() {
            all_messages.push(json!({"role": "system", "content": prompt}));
        }
        all_messages.extend(prefixed_messages);

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
        let mut task_follow_up_rounds = 0usize;
        let mut task_follow_up_mode: Option<TaskTurnFollowUpMode> = None;
        let mut task_follow_up_locale: Option<InternalContextLocale> = None;
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
            let runtime_guidance_messages = drain_runtime_guidance_messages(
                session_id.as_deref(),
                turn_id.as_deref(),
                &callbacks,
            )
            .await;
            if !runtime_guidance_messages.is_empty() {
                api_messages.extend(runtime_guidance_messages);
            }
            api_messages = sanitize_messages_for_request(api_messages);
            if let Some(cb) = &callbacks.on_before_model_request {
                cb(
                    api_messages.clone().into(),
                    None,
                    None,
                );
            }

            let mut resp = None;
            let mut last_err: Option<String> = None;
            let mut remote_active_summary_attempted = false;
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
                        if matches!(task_follow_up_mode, Some(TaskTurnFollowUpMode::ReviewExecution)) {
                            StreamCallbacks {
                                on_chunk: None,
                                on_thinking: None,
                            }
                        } else {
                            StreamCallbacks {
                                on_chunk: callbacks.on_chunk.clone(),
                                on_thinking: callbacks.on_thinking.clone(),
                            }
                        },
                        reasoning_enabled,
                        provider.clone(),
                        thinking_level.clone(),
                        session_id.clone(),
                        turn_id.clone(),
                        message_mode.clone(),
                        message_source.clone(),
                        follow_up_request_metadata(
                            task_follow_up_mode,
                            iteration,
                        ),
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
                        if is_token_limit_error(&err) {
                            if !remote_active_summary_attempted
                                && resolved_purpose_allows_active_summary(purpose.as_str())
                            {
                                remote_active_summary_attempted = true;
                                if self
                                    .wait_for_remote_active_summary_and_refresh(
                                        session_id.as_deref(),
                                        reasoning_enabled,
                                        &callbacks,
                                        &mut messages,
                                    )
                                    .await
                                {
                                    api_messages = sanitize_messages_for_request(messages.clone());
                                    continue;
                                }
                            }
                        }
                        match handle_transient_retry(
                            "[AI_V2]",
                            &err,
                            &mut transient_retry_count,
                            max_transient_retries,
                        )
                        .await
                        {
                            Ok(true) => continue,
                            Err(error_message) => {
                                last_err = Some(error_message);
                            }
                            Ok(false) => {}
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

            if matches!(task_follow_up_mode, Some(TaskTurnFollowUpMode::ReviewExecution)) {
                let review_locale = task_follow_up_locale
                    .take()
                    .unwrap_or(InternalContextLocale::ZhCn);
                match parse_task_turn_review_outcome(resp.content.as_str()) {
                    TaskTurnReviewOutcome::Pass => {
                        let final_content = last_visible_completion_content
                            .clone()
                            .unwrap_or_else(|| strip_task_turn_review_marker(resp.content.as_str()));
                        let final_reasoning = last_visible_completion_reasoning
                            .clone()
                            .or(resp.reasoning.clone());
                        let final_finish_reason = last_visible_completion_finish_reason
                            .clone()
                            .or(resp.finish_reason.clone());
                        return Ok(build_ai_client_success_payload(
                            final_content,
                            final_reasoning,
                            final_finish_reason,
                            iteration,
                        ));
                    }
                    TaskTurnReviewOutcome::NeedsMoreWork | TaskTurnReviewOutcome::Unknown => {
                        if task_follow_up_rounds < MAX_TASK_FOLLOW_UP_ROUNDS {
                            task_follow_up_rounds += 1;
                            if let Some(cb) = &callbacks.on_thinking {
                                cb("复查发现仍需处理，继续同一轮修正。".to_string());
                            }
                            messages = build_task_turn_follow_up_message(
                                build_task_turn_review_retry_guidance(review_locale).as_str(),
                            )
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                            task_follow_up_mode = Some(TaskTurnFollowUpMode::ContinueExecution);
                            iteration += 1;
                            continue;
                        }
                    }
                }
            }

            let Some(tool_calls_val) = resp.tool_calls.clone().filter(|tool_calls| {
                tool_calls
                    .as_array()
                    .map(|items| !items.is_empty())
                    .unwrap_or(false)
            }) else {
                if let (Some(sid), Some(tid)) = (
                    session_id.as_deref(),
                    turn_id.as_deref(),
                ) {
                    if task_follow_up_rounds < MAX_TASK_FOLLOW_UP_ROUNDS {
                        if let Some(directive) =
                            build_task_turn_follow_up_directive(sid, tid).await
                        {
                            last_visible_completion_content = Some(resp.content.clone());
                            last_visible_completion_reasoning = resp.reasoning.clone();
                            last_visible_completion_finish_reason = resp.finish_reason.clone();
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
                            messages = build_task_turn_follow_up_message(directive.guidance.as_str())
                                .as_array()
                                .cloned()
                                .unwrap_or_default();
                            iteration += 1;
                            continue;
                        }
                    }
                }
                return Ok(build_ai_client_success_payload(
                    resp.content,
                    resp.reasoning,
                    resp.finish_reason,
                    iteration,
                ));
            };

            let tool_calls_arr = tool_calls_val.as_array().cloned().unwrap_or_default();
            let mcp_tool_execute = self.mcp_tool_execute.clone();
            let message_manager = self.message_manager.clone();
            let persist_session_id = session_id.clone();
            let tool_results = execute_tool_lifecycle(
                tool_calls_arr.as_slice(),
                tool_calls_val.clone(),
                session_id.as_deref(),
                persist_tool_messages,
                &callbacks,
                |on_tools_stream_cb| {
                    mcp_tool_execute.execute_tools_stream(
                        &tool_calls_arr,
                        session_id.as_deref(),
                        turn_id.as_deref(),
                        Some(model.as_str()),
                        on_tools_stream_cb,
                    )
                },
                |results| results.to_vec(),
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
            .tool_results;

            let mut new_messages = api_messages.clone();
            let assistant_msg = build_assistant_message_with_parts(
                if resp.content.is_empty() {
                    Value::Null
                } else {
                    Value::String(resp.content.clone())
                },
                if reasoning_enabled {
                    resp.reasoning.as_deref()
                } else {
                    None
                },
                reasoning_enabled,
                resp.tool_calls.clone(),
            );
            new_messages.push(assistant_msg);

            for result in &tool_results {
                new_messages.push(build_tool_role_message(
                    result.tool_call_id.as_str(),
                    cap_tool_content_for_input(result.content.as_str()).as_str(),
                ));
            }

            messages = new_messages;
            iteration += 1;
        }
    }

    async fn wait_for_remote_active_summary_and_refresh(
        &self,
        session_id: Option<&str>,
        include_reasoning: bool,
        callbacks: &AiClientCallbacks,
        messages: &mut Vec<Value>,
    ) -> bool {
        let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        let Ok(Some(session)) =
            crate::services::chatos_sessions::get_session_by_id(session_id).await
        else {
            return false;
        };
        let Some(status) =
            chatos_memory_engine::try_start_chatos_active_summary(&session, "context_overflow")
                .await
        else {
            info!(
                "[AI_V2] active summary trigger unavailable: session_id={}",
                session_id
            );
            return false;
        };

        if status.failed || (!status.running && !status.generated && !status.compacted) {
            info!(
                "[AI_V2] active summary trigger did not enter running state: session_id={} failed={} running={} generated={} compacted={}",
                session_id,
                status.failed,
                status.running,
                status.generated,
                status.compacted
            );
            return false;
        }

        info!(
            "[AI_V2] active summary started: session_id={} job_run_id={} pending_before_count={}",
            session_id,
            status.job_run_id.as_deref().unwrap_or("-"),
            status.pending_before_count.unwrap_or(0)
        );
        notify_active_summary_progress(
            callbacks,
            "正在自动压缩上下文，压缩完成后将继续当前请求。",
            &session,
            &status,
        );

        let completed =
            match chatos_memory_engine::wait_for_existing_chatos_active_summary_completion(
                &session, status,
            )
            .await
            {
                Ok(status) => status,
                Err(err) => {
                    info!(
                        "[AI_V2] active summary wait failed: session_id={} error={}",
                        session_id, err
                    );
                    return false;
                }
            };

        if completed.failed || (!completed.generated && !completed.compacted) {
            info!(
                "[AI_V2] active summary completed without compaction: session_id={} failed={} generated={} compacted={}",
                session_id,
                completed.failed,
                completed.generated,
                completed.compacted
            );
            return false;
        }

        notify_active_summary_progress(
            callbacks,
            "上下文压缩完成，正在继续当前请求。",
            &session,
            &completed,
        );

        self.refresh_context_from_memory(Some(session_id), include_reasoning, messages)
            .await;
        true
    }
}

fn follow_up_request_metadata(
    mode: Option<TaskTurnFollowUpMode>,
    iteration: i64,
) -> Option<Value> {
    mode.map(|mode| {
        let mut metadata = match mode {
            TaskTurnFollowUpMode::ReviewExecution => {
                build_hidden_task_turn_review_metadata()
                    .as_object()
                    .cloned()
                    .unwrap_or_default()
            }
            TaskTurnFollowUpMode::ContinueExecution => serde_json::Map::new(),
        };
        metadata.insert(
            TASK_FOLLOW_UP_ROLE_METADATA_KEY.to_string(),
            Value::String(match mode {
                TaskTurnFollowUpMode::ContinueExecution => "continue".to_string(),
                TaskTurnFollowUpMode::ReviewExecution => "review".to_string(),
            }),
        );
        metadata.insert(
            "task_follow_up_iteration".to_string(),
            Value::Number(iteration.into()),
        );
        Value::Object(metadata)
    })
}

fn resolved_purpose_allows_active_summary(purpose: &str) -> bool {
    purpose == "chat"
}

fn notify_active_summary_progress(
    callbacks: &AiClientCallbacks,
    message: &str,
    session: &Session,
    status: &memory_engine_sdk::RunThreadActiveSummaryResponse,
) {
    if let Some(cb) = &callbacks.on_thinking {
        cb(message.to_string());
    }
    if let Some(cb) = &callbacks.on_context_summarized_start {
        cb(json!({
            "kind": "active_summary_progress",
            "message": message,
            "session_id": session.id,
            "job_run_id": status.job_run_id,
            "pending_before_count": status.pending_before_count,
            "pending_after_count": status.pending_after_count,
            "running": status.running,
            "completed": status.completed,
            "failed": status.failed,
            "generated": status.generated,
            "compacted": status.compacted
        }));
    }
}

async fn drain_runtime_guidance_messages(
    session_id: Option<&str>,
    turn_id: Option<&str>,
    callbacks: &AiClientCallbacks,
) -> Vec<Value> {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let Some(turn_id) = turn_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };

    let drained = drain_runtime_guidance_items(Some(session_id), Some(turn_id));
    if drained.is_empty() {
        return Vec::new();
    }

    let mut messages = Vec::with_capacity(drained.len());
    for drained_item in drained {
        let locale = resolve_runtime_guidance_locale(&drained_item.guidance_item).await;
        messages.push(build_runtime_guidance_message(
            &drained_item.guidance_item,
            locale,
        ));
        if let Some(applied_item) = drained_item.applied_item {
            if let Some(cb) = &callbacks.on_runtime_guidance_applied {
                cb(build_runtime_guidance_applied_event(
                    &applied_item,
                    drained_item.pending_count,
                    false,
                ));
            }
        }
    }

    messages
}

fn build_runtime_guidance_message(
    guidance_item: &RuntimeGuidanceItem,
    locale: InternalContextLocale,
) -> Value {
    json!({
        "role": "system",
        "content": format_runtime_guidance_instruction(guidance_item, locale),
    })
}

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) {
            self.max_iterations = v;
        }
    }
}
