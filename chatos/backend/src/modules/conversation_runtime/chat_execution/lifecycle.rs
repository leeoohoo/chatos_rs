use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_ai_runtime::{
    AiResponse, RuntimeBeforeModelRequest, RuntimeCallbacks, RuntimeFinalResponseAction,
    RuntimeFinalResponseContext, RuntimeIterationContext, RuntimeLifecycleHook,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::internal_context_locale::InternalContextLocale;
use crate::modules::conversation_runtime::project_execution_planner::{
    materialization_succeeded as project_execution_planner_terminal_tool_succeeded,
    FINALIZATION_PROMPT as PROJECT_EXECUTION_PLANNER_FINALIZATION_PROMPT,
};
use crate::modules::conversation_runtime::project_planning_delegation::{
    background_wait_succeeded as project_planning_background_wait_succeeded,
    task_creation_succeeded as project_planning_task_creation_succeeded,
    FINALIZATION_PROMPT as PROJECT_PLANNING_DELEGATION_FINALIZATION_PROMPT,
};
use crate::modules::conversation_runtime::task_board::{
    build_task_turn_follow_up_directive, build_task_turn_follow_up_message,
    build_task_turn_review_retry_guidance, parse_task_turn_review_outcome,
    strip_task_turn_review_marker, TaskTurnFollowUpMode, TaskTurnReviewOutcome,
};
use crate::services::ai_client_common::AiClientCallbacks;

use super::system_input_item;

pub(crate) struct ChatosRuntimeLifecycleHook {
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) model_name: String,
    pub(crate) supports_images: Option<bool>,
    pub(crate) callbacks: AiClientCallbacks,
    pub(crate) max_task_follow_up_rounds: usize,
    pub(crate) task_turn: Arc<Mutex<TaskTurnLifecycleState>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct TaskTurnLifecycleState {
    pub(crate) follow_up_rounds: usize,
    pub(crate) mode: Option<TaskTurnFollowUpMode>,
    pub(crate) last_visible_response: Option<AiResponse>,
    pub(crate) review_locale: Option<InternalContextLocale>,
    pub(crate) review_attempted: bool,
    pub(crate) review_last_outcome: Option<TaskTurnReviewOutcome>,
    pub(crate) continuation_history: Vec<Value>,
    pub(crate) project_execution_plan_materialized: bool,
    pub(crate) project_planning_integrity_guard: bool,
    #[serde(default)]
    pub(crate) project_planning_task_created: bool,
    #[serde(default)]
    pub(crate) project_planning_background_acknowledged: bool,
    #[serde(default)]
    pub(crate) project_planning_delegation_repair_rounds: usize,
    pub(crate) project_planning_write_failures: Vec<String>,
    pub(crate) project_planning_repair_rounds: usize,
    pub(crate) project_planning_repair_mutation_succeeded: bool,
    pub(crate) project_planning_pending_dependency_batch_signature: Option<String>,
    pub(crate) project_planning_dependency_write_cycle: Vec<String>,
    pub(crate) project_planning_last_verified_dependency_cycle: Vec<String>,
    pub(crate) project_planning_force_finalization: bool,
}

const MAX_PROJECT_PLANNING_REPAIR_ROUNDS: usize = 3;
const PROJECT_PLANNING_LOOP_FINALIZATION_PROMPT: &str = "[Project Planning Finalization]\nThe program detected that an identical project-task dependency mutation batch succeeded repeatedly and an authoritative dependency-graph read completed afterward. Do not call any more tools. Summarize the latest verified project state for the user. Do not claim work that is absent from the latest graph; if anything remains incomplete, state the concrete gap instead of attempting another identical write.";

fn is_project_planning_mutation_tool(name: &str) -> bool {
    matches!(
        name,
        "project_management_service_initialize_project"
            | "project_management_service_create_requirement"
            | "project_management_service_update_requirement"
            | "project_management_service_delete_requirement"
            | "project_management_service_set_requirement_dependencies"
            | "project_management_service_upsert_requirement_technical_document"
            | "project_management_service_create_project_task"
            | "project_management_service_update_project_task"
            | "project_management_service_delete_project_task"
            | "project_management_service_set_project_task_dependencies"
    )
}

fn successful_tool_result(result: &Value, expected_name: &str) -> bool {
    result.get("name").and_then(Value::as_str) == Some(expected_name)
        && result.get("success").and_then(Value::as_bool) == Some(true)
        && result.get("is_error").and_then(Value::as_bool) != Some(true)
}

fn planning_failure_summary(result: &Value) -> Option<String> {
    let name = result.get("name").and_then(Value::as_str)?;
    if !is_project_planning_mutation_tool(name)
        || (result.get("success").and_then(Value::as_bool) == Some(true)
            && result.get("is_error").and_then(Value::as_bool) != Some(true))
    {
        return None;
    }
    let detail = result
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("项目规划写入失败");
    Some(format!(
        "{name}: {}",
        detail.chars().take(500).collect::<String>()
    ))
}

fn canonical_json_signature(value: &Value) -> String {
    match value {
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json_signature)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            format!(
                "{{{}}}",
                entries
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_else(|_| key.clone()),
                        canonical_json_signature(value)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        _ => value.to_string(),
    }
}

fn project_dependency_write_batch_signature(tool_calls: &Value) -> Option<String> {
    let mut signatures = tool_calls
        .as_array()?
        .iter()
        .filter_map(|call| {
            let function = call.get("function").unwrap_or(call);
            if function.get("name").and_then(Value::as_str)
                != Some("project_management_service_set_project_task_dependencies")
            {
                return None;
            }
            let arguments = function
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let arguments = arguments
                .as_str()
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or(arguments);
            Some(canonical_json_signature(&arguments))
        })
        .collect::<Vec<_>>();
    if signatures.is_empty() {
        return None;
    }
    signatures.sort();
    Some(signatures.join("\n"))
}

fn verified_dependency_cycle_repeats(signatures: &[String]) -> bool {
    let mut unique = signatures.to_vec();
    unique.sort();
    unique.dedup();
    unique.len() < signatures.len()
}

pub(crate) fn track_project_planning_integrity(
    mut callbacks: RuntimeCallbacks,
    state: Arc<Mutex<TaskTurnLifecycleState>>,
) -> RuntimeCallbacks {
    let downstream_start = callbacks.on_tools_start.clone();
    callbacks.on_tools_start = Some(Arc::new({
        let state = Arc::clone(&state);
        move |payload| {
            if let Ok(mut state) = state.lock() {
                if state.project_planning_integrity_guard {
                    state.project_planning_pending_dependency_batch_signature =
                        project_dependency_write_batch_signature(&payload);
                }
            }
            if let Some(callback) = downstream_start.as_ref() {
                callback(payload);
            }
        }
    }));
    let downstream_end = callbacks.on_tools_end.clone();
    callbacks.on_tools_end = Some(Arc::new(move |payload| {
        let tool_results = payload
            .get("tool_results")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if let Ok(mut state) = state.lock() {
            if state.project_planning_integrity_guard {
                if project_planning_task_creation_succeeded(&payload) {
                    state.project_planning_task_created = true;
                }
                if state.project_planning_task_created
                    && project_planning_background_wait_succeeded(&payload)
                {
                    state.project_planning_background_acknowledged = true;
                }
                let dependency_batch_signature = state
                    .project_planning_pending_dependency_batch_signature
                    .take();
                let failures = tool_results
                    .iter()
                    .filter_map(planning_failure_summary)
                    .collect::<Vec<_>>();
                if !failures.is_empty() {
                    state.project_planning_write_failures = failures;
                    state.project_planning_repair_mutation_succeeded = false;
                    state.project_planning_dependency_write_cycle.clear();
                } else {
                    if let Some(signature) = dependency_batch_signature.as_ref() {
                        let successful_dependency_writes = tool_results
                            .iter()
                            .filter(|result| {
                                successful_tool_result(
                                    result,
                                    "project_management_service_set_project_task_dependencies",
                                )
                            })
                            .count();
                        let expected_dependency_writes = signature.lines().count();
                        if successful_dependency_writes == expected_dependency_writes {
                            state
                                .project_planning_dependency_write_cycle
                                .push(signature.clone());
                        }
                    }

                    let graph_verified_in_later_batch = dependency_batch_signature.is_none()
                        && tool_results.iter().any(|result| {
                            successful_tool_result(
                                result,
                                "project_management_service_get_project_dependency_graph",
                            )
                        });
                    if graph_verified_in_later_batch
                        && !state.project_planning_dependency_write_cycle.is_empty()
                    {
                        let repeated_within_cycle = verified_dependency_cycle_repeats(
                            state.project_planning_dependency_write_cycle.as_slice(),
                        );
                        let mut verified_cycle =
                            state.project_planning_dependency_write_cycle.clone();
                        verified_cycle.sort();
                        verified_cycle.dedup();
                        let repeated_verified_cycle = !state
                            .project_planning_last_verified_dependency_cycle
                            .is_empty()
                            && state.project_planning_last_verified_dependency_cycle
                                == verified_cycle;
                        state.project_planning_force_finalization =
                            repeated_within_cycle || repeated_verified_cycle;
                        state.project_planning_last_verified_dependency_cycle = verified_cycle;
                        state.project_planning_dependency_write_cycle.clear();
                    }

                    if !state.project_planning_write_failures.is_empty() {
                        let verified_after_prior_repair = state
                            .project_planning_repair_mutation_succeeded
                            && tool_results.iter().any(|result| {
                                successful_tool_result(
                                    result,
                                    "project_management_service_get_project_dependency_graph",
                                )
                            });
                        if verified_after_prior_repair {
                            state.project_planning_write_failures.clear();
                            state.project_planning_repair_mutation_succeeded = false;
                        } else if tool_results.iter().any(|result| {
                            result
                                .get("name")
                                .and_then(Value::as_str)
                                .is_some_and(is_project_planning_mutation_tool)
                                && result.get("success").and_then(Value::as_bool) == Some(true)
                                && result.get("is_error").and_then(Value::as_bool) != Some(true)
                        }) {
                            state.project_planning_repair_mutation_succeeded = true;
                        }
                    }
                }
            }
        }
        if let Some(callback) = downstream_end.as_ref() {
            callback(payload);
        }
    }));
    callbacks
}

pub(crate) fn track_project_execution_planner_completion(
    mut callbacks: RuntimeCallbacks,
    state: Arc<Mutex<TaskTurnLifecycleState>>,
) -> RuntimeCallbacks {
    let downstream = callbacks.on_tools_end.clone();
    callbacks.on_tools_end = Some(Arc::new(move |payload| {
        if project_execution_planner_terminal_tool_succeeded(&payload) {
            if let Ok(mut state) = state.lock() {
                state.project_execution_plan_materialized = true;
                state.mode = None;
            }
        }
        if let Some(callback) = downstream.as_ref() {
            callback(payload);
        }
    }));
    callbacks
}

impl ChatosRuntimeLifecycleHook {
    pub(super) fn task_turn_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, TaskTurnLifecycleState>, String> {
        self.task_turn
            .lock()
            .map_err(|_| "task turn lifecycle state lock poisoned".to_string())
    }

    fn emit_task_turn_phase(
        &self,
        phase: &'static str,
        mode: TaskTurnFollowUpMode,
        iteration: usize,
    ) {
        if let Some(callback) = &self.callbacks.on_turn_phase {
            callback(json!({
                "phase": phase,
                "reason": "task_follow_up",
                "task_follow_up_mode": match mode {
                    TaskTurnFollowUpMode::ContinueExecution => "continue",
                    TaskTurnFollowUpMode::ReviewExecution => "review",
                },
                "iteration": iteration,
            }));
        }
    }

    fn emit_task_turn_thinking(&self, mode: TaskTurnFollowUpMode) {
        if let Some(callback) = &self.callbacks.on_thinking {
            callback(match mode {
                TaskTurnFollowUpMode::ContinueExecution => {
                    "检测到尚未完成的任务，继续在同一轮执行。".to_string()
                }
                TaskTurnFollowUpMode::ReviewExecution => {
                    "任务看起来已完成，正在同一轮进行复查。".to_string()
                }
            });
        }
    }

    fn continue_with_response(
        state: &mut TaskTurnLifecycleState,
        response: &AiResponse,
        guidance: &str,
    ) -> Vec<Value> {
        if let Some(item) = assistant_response_input_item(response) {
            state.continuation_history.push(item);
        }
        state
            .continuation_history
            .extend(follow_up_message_items(guidance));
        state.continuation_history.clone()
    }

    fn handle_review_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        let outcome = parse_task_turn_review_outcome(context.response.content.as_str());
        let mut state = self.task_turn_state()?;
        state.review_attempted = true;
        state.review_last_outcome = Some(outcome);

        if outcome == TaskTurnReviewOutcome::Pass {
            let replacement = state
                .last_visible_response
                .clone()
                .unwrap_or_else(|| AiResponse {
                    content: strip_task_turn_review_marker(context.response.content.as_str()),
                    ..context.response.clone()
                });
            state.mode = None;
            return Ok(RuntimeFinalResponseAction::Replace(Box::new(replacement)));
        }

        if state.follow_up_rounds >= self.max_task_follow_up_rounds {
            state.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        }

        let locale = state.review_locale.unwrap_or(InternalContextLocale::ZhCn);
        state.follow_up_rounds += 1;
        state.mode = Some(TaskTurnFollowUpMode::ContinueExecution);
        let guidance = build_task_turn_review_retry_guidance(locale);
        let input_items =
            Self::continue_with_response(&mut state, &context.response, guidance.as_str());
        drop(state);

        self.emit_task_turn_phase(
            "execution",
            TaskTurnFollowUpMode::ContinueExecution,
            context.iteration,
        );
        self.emit_task_turn_thinking(TaskTurnFollowUpMode::ContinueExecution);
        Ok(RuntimeFinalResponseAction::Continue {
            input_items,
            reason: "task_review_retry".to_string(),
        })
    }

    fn repair_failed_project_planning_writes(
        &self,
        context: &RuntimeFinalResponseContext,
    ) -> Result<Option<RuntimeFinalResponseAction>, String> {
        let mut state = self.task_turn_state()?;
        if !state.project_planning_integrity_guard
            || state.project_planning_write_failures.is_empty()
        {
            return Ok(None);
        }
        if state.project_planning_repair_rounds >= MAX_PROJECT_PLANNING_REPAIR_ROUNDS {
            let failures = state.project_planning_write_failures.join(" | ");
            return Err(format!(
                "项目规划仍有未修复的写入失败，不能标记为完成：{failures}"
            ));
        }

        state.project_planning_repair_rounds += 1;
        let guidance = if state.project_planning_repair_mutation_succeeded {
            "[Project Planning Integrity Guard]\n程序检测到此前失败的规划写入已有修复动作，但尚未通过后续权威依赖图验证。不要总结完成。现在重新读取项目任务和项目依赖图；若仍有缺口，继续修复。只有验证结果与最终总结一致后才能结束本轮。"
        } else {
            "[Project Planning Integrity Guard]\n程序检测到本轮至少一个项目规划写入失败。不要总结完成，也不要重构、缩写或猜测任何 ID。先重新读取权威项目任务和依赖图，复制工具返回的精确 ID，修复所有失败写入；修复后必须在下一批工具调用中再次读取项目依赖图验证。"
        };
        let input_items = Self::continue_with_response(&mut state, &context.response, guidance);
        drop(state);
        self.emit_task_turn_thinking(TaskTurnFollowUpMode::ContinueExecution);
        Ok(Some(RuntimeFinalResponseAction::Continue {
            input_items,
            reason: "project_planning_integrity_repair".to_string(),
        }))
    }

    fn require_project_planning_delegation(
        &self,
        context: &RuntimeFinalResponseContext,
    ) -> Result<Option<RuntimeFinalResponseAction>, String> {
        let mut state = self.task_turn_state()?;
        if !state.project_planning_integrity_guard
            || (state.project_planning_task_created
                && state.project_planning_background_acknowledged)
        {
            return Ok(None);
        }
        if state.project_planning_delegation_repair_rounds >= MAX_PROJECT_PLANNING_REPAIR_ROUNDS {
            let missing = if state.project_planning_task_created {
                "规划任务已经创建，但未完成 wait_for_task_completion 的后台执行确认"
            } else {
                "未创建 Task Runner 规划任务"
            };
            return Err(format!("规划模式委派校验失败，不能标记为完成：{missing}"));
        }

        state.project_planning_delegation_repair_rounds += 1;
        let guidance = if state.project_planning_task_created {
            "[Planning Delegation Guard]\n程序已确认规划任务创建成功，但尚未确认后台执行已被接管。不要输出规划内容或声称规划完成。现在必须调用 wait_for_task_completion 等待刚创建的任务进入正常后台回传流程；成功后只向用户简短确认规划已经开始。"
        } else {
            "[Planning Delegation Guard]\n当前是程序开启的规划模式，但本轮尚未创建 Task Runner 规划任务。不要用自由文本规划代替任务，也不要声称需求文档、技术文档或项目任务已经生成。现在必须创建 requires_execution=false 的规划任务，要求规划 Agent 生成并复核需求及验收条件、非空技术文档、项目任务和依赖关系；创建成功后必须调用 wait_for_task_completion。"
        };
        let input_items = Self::continue_with_response(&mut state, &context.response, guidance);
        drop(state);
        self.emit_task_turn_thinking(TaskTurnFollowUpMode::ContinueExecution);
        Ok(Some(RuntimeFinalResponseAction::Continue {
            input_items,
            reason: "project_planning_delegation_repair".to_string(),
        }))
    }
}

#[async_trait]
impl RuntimeLifecycleHook for ChatosRuntimeLifecycleHook {
    async fn before_model_request(
        &self,
        _context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        let mut input_items =
            crate::services::runtime_guidance_input::load_runtime_guidance_input_items(
                Some(self.session_id.as_str()),
                Some(self.turn_id.as_str()),
                false,
                self.model_name.as_str(),
                self.supports_images,
                &self.callbacks,
            )
            .await;
        let state = self.task_turn_state()?;
        let project_execution_plan_materialized = state.project_execution_plan_materialized;
        let project_planning_delegation_finalized = state.project_planning_integrity_guard
            && state.project_planning_task_created
            && state.project_planning_background_acknowledged
            && state.project_planning_write_failures.is_empty();
        let project_planning_force_finalization =
            state.project_planning_force_finalization && project_planning_delegation_finalized;
        let review_mode = matches!(state.mode, Some(TaskTurnFollowUpMode::ReviewExecution));
        drop(state);
        if project_execution_plan_materialized {
            input_items.push(system_input_item(
                PROJECT_EXECUTION_PLANNER_FINALIZATION_PROMPT,
            ));
        }
        if project_planning_force_finalization && !project_planning_delegation_finalized {
            input_items.push(system_input_item(PROJECT_PLANNING_LOOP_FINALIZATION_PROMPT));
        }
        if project_planning_delegation_finalized {
            input_items.push(system_input_item(
                PROJECT_PLANNING_DELEGATION_FINALIZATION_PROMPT,
            ));
        }
        Ok(RuntimeBeforeModelRequest::unchanged()
            .with_input_items(input_items)
            .with_stream_output(!review_mode)
            .with_tools_enabled(
                !review_mode
                    && !project_execution_plan_materialized
                    && !project_planning_force_finalization
                    && !project_planning_delegation_finalized,
            ))
    }

    async fn after_final_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        if self.task_turn_state()?.project_execution_plan_materialized {
            self.task_turn_state()?.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        }
        let should_force_finalize = {
            let state = self.task_turn_state()?;
            state.project_planning_force_finalization
                && state.project_planning_write_failures.is_empty()
                && state.project_planning_task_created
                && state.project_planning_background_acknowledged
        };
        if should_force_finalize {
            self.task_turn_state()?.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        }
        if let Some(action) = self.repair_failed_project_planning_writes(&context)? {
            return Ok(action);
        }
        let delegation_finalized = {
            let state = self.task_turn_state()?;
            state.project_planning_integrity_guard
                && state.project_planning_task_created
                && state.project_planning_background_acknowledged
        };
        if delegation_finalized {
            self.task_turn_state()?.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        }
        if let Some(action) = self.require_project_planning_delegation(&context)? {
            return Ok(action);
        }
        if matches!(
            self.task_turn_state()?.mode,
            Some(TaskTurnFollowUpMode::ReviewExecution)
        ) {
            return self.handle_review_response(context);
        }

        if self.max_task_follow_up_rounds == 0 {
            return Ok(RuntimeFinalResponseAction::Accept);
        }

        let Some(directive) =
            build_task_turn_follow_up_directive(self.session_id.as_str(), self.turn_id.as_str())
                .await
        else {
            self.task_turn_state()?.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        };

        let mut state = self.task_turn_state()?;
        if state.follow_up_rounds >= self.max_task_follow_up_rounds {
            state.mode = None;
            return Ok(RuntimeFinalResponseAction::Accept);
        }
        state.last_visible_response = Some(context.response.clone());
        state.follow_up_rounds += 1;
        state.mode = Some(directive.mode);
        state.review_locale = Some(directive.locale);
        let input_items = Self::continue_with_response(
            &mut state,
            &context.response,
            directive.guidance.as_str(),
        );
        drop(state);

        let phase = match directive.mode {
            TaskTurnFollowUpMode::ContinueExecution => "execution",
            TaskTurnFollowUpMode::ReviewExecution => "review",
        };
        self.emit_task_turn_phase(phase, directive.mode, context.iteration);
        self.emit_task_turn_thinking(directive.mode);
        Ok(RuntimeFinalResponseAction::Continue {
            input_items,
            reason: match directive.mode {
                TaskTurnFollowUpMode::ContinueExecution => "task_follow_up".to_string(),
                TaskTurnFollowUpMode::ReviewExecution => "task_review".to_string(),
            },
        })
    }

    async fn final_response_metadata(
        &self,
        _context: RuntimeFinalResponseContext,
    ) -> Result<Option<Value>, String> {
        let state = self.task_turn_state()?;
        Ok(Some(task_turn_review_metadata(&state)))
    }
}

pub(crate) fn task_turn_review_metadata(state: &TaskTurnLifecycleState) -> Value {
    let outcome = match state.review_last_outcome {
        Some(TaskTurnReviewOutcome::Pass) => "pass",
        Some(TaskTurnReviewOutcome::NeedsMoreWork) => "needs_more_work",
        Some(TaskTurnReviewOutcome::Unknown) => "unknown",
        None => "not_attempted",
    };
    json!({
        "task_turn_review": {
            "attempted": state.review_attempted,
            "outcome": outcome,
            "rounds": state.follow_up_rounds,
        },
        "project_planning_integrity": {
            "guarded": state.project_planning_integrity_guard,
            "planning_task_created": state.project_planning_task_created,
            "background_acknowledged": state.project_planning_background_acknowledged,
            "delegation_repair_rounds": state.project_planning_delegation_repair_rounds,
            "pending_write_failure_count": state.project_planning_write_failures.len(),
            "repair_rounds": state.project_planning_repair_rounds,
        }
    })
}

pub(super) fn assistant_response_input_item(response: &AiResponse) -> Option<Value> {
    let content = if response.content.trim().is_empty() {
        response.reasoning.as_deref().unwrap_or("").trim()
    } else {
        response.content.trim()
    };
    if content.is_empty() {
        return None;
    }
    Some(json!({
        "type": "message",
        "role": "assistant",
        "content": [{ "type": "output_text", "text": content }],
    }))
}

fn follow_up_message_items(guidance: &str) -> Vec<Value> {
    match build_task_turn_follow_up_message(guidance) {
        Value::Array(items) => items,
        Value::Null => Vec::new(),
        item => vec![item],
    }
}
