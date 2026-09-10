use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_ai_runtime::{
    AiResponse, RuntimeBeforeModelRequest, RuntimeFinalResponseAction, RuntimeFinalResponseContext,
    RuntimeIterationContext, RuntimeLifecycleHook,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::internal_context_locale::InternalContextLocale;
use crate::modules::conversation_runtime::task_board::{
    build_task_turn_follow_up_directive, build_task_turn_follow_up_message,
    build_task_turn_review_retry_guidance, parse_task_turn_review_outcome,
    strip_task_turn_review_marker, TaskTurnFollowUpMode, TaskTurnReviewOutcome,
};
use crate::services::ai_client_common::AiClientCallbacks;

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
}

#[async_trait]
impl RuntimeLifecycleHook for ChatosRuntimeLifecycleHook {
    async fn before_model_request(
        &self,
        _context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        let input_items =
            crate::services::runtime_guidance_input::load_runtime_guidance_input_items(
                Some(self.session_id.as_str()),
                Some(self.turn_id.as_str()),
                false,
                self.model_name.as_str(),
                self.supports_images,
                &self.callbacks,
            )
            .await;
        let review_mode = matches!(
            self.task_turn_state()?.mode,
            Some(TaskTurnFollowUpMode::ReviewExecution)
        );
        Ok(RuntimeBeforeModelRequest::unchanged()
            .with_input_items(input_items)
            .with_stream_output(!review_mode)
            .with_tools_enabled(!review_mode))
    }

    async fn after_final_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
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
