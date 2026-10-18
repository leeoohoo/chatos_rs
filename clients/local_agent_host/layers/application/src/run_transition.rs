// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{new_event_id, requirement_survey_runtime, LocalAgentRuntimeError};
use chatos_local_agent_ports::RunTransition;
use chatos_local_agent_protocol::{
    LocalAgentRunRecord, LocalAgentRunStatus, LocalAgentStepOutcome, LocalAgentToolBatch,
};
use serde_json::json;

pub(super) fn transition_for_outcome(
    run: &LocalAgentRunRecord,
    claim_token: String,
    expected_version: u64,
    outcome: LocalAgentStepOutcome,
    now: i64,
) -> Result<RunTransition, LocalAgentRuntimeError> {
    requirement_survey_runtime::validate_tool_batch(&outcome)?;
    let tool_batch = match &outcome {
        LocalAgentStepOutcome::WaitForTool {
            batch_id,
            tool_calls,
            ..
        } => Some(LocalAgentToolBatch {
            batch_id: batch_id.clone(),
            calls: tool_calls.clone(),
        }),
        _ => None,
    };
    let checkpoint = match &outcome {
        LocalAgentStepOutcome::Continue { checkpoint }
        | LocalAgentStepOutcome::WaitForTool { checkpoint, .. }
        | LocalAgentStepOutcome::WaitForUser { checkpoint, .. } => Some(checkpoint.clone()),
        _ => None,
    };
    let next_model_attempt = match &outcome {
        LocalAgentStepOutcome::Retry {
            next_model_attempt, ..
        } => *next_model_attempt,
        _ => 1,
    };
    let clear_continuation_input = !matches!(&outcome, LocalAgentStepOutcome::Retry { .. });
    let (next_status, next_attempt, pending_tool_batch, terminal_outcome, event_type, payload) =
        match outcome {
            LocalAgentStepOutcome::Continue { checkpoint }
                if run.iteration >= run.max_iterations =>
            {
                (
                    LocalAgentRunStatus::NeedsReview,
                    None,
                    None,
                    None,
                    "iteration_limit_reached",
                    json!({
                        "reason": "maximum model iterations reached",
                        "checkpoint": checkpoint
                    }),
                )
            }
            LocalAgentStepOutcome::Continue { checkpoint } => (
                LocalAgentRunStatus::ContinuationReady,
                None,
                None,
                None,
                "continuation_requested",
                json!({"checkpoint": checkpoint}),
            ),
            LocalAgentStepOutcome::WaitForTool {
                batch_id,
                tool_calls,
                ..
            } => {
                let batch = json!({"batch_id": batch_id, "tool_calls": tool_calls});
                (
                    LocalAgentRunStatus::WaitingToolResult,
                    None,
                    Some(batch.clone()),
                    None,
                    "tool_batch_requested",
                    batch,
                )
            }
            LocalAgentStepOutcome::WaitForUser { prompt, .. } => (
                LocalAgentRunStatus::WaitingUser,
                None,
                None,
                None,
                "user_input_requested",
                json!({"prompt": prompt}),
            ),
            LocalAgentStepOutcome::Retry {
                resume_at_unix_ms,
                next_model_attempt,
                reason,
            } => (
                LocalAgentRunStatus::RetryScheduled,
                Some(resume_at_unix_ms),
                None,
                None,
                "retry_scheduled",
                json!({
                    "resume_at_unix_ms": resume_at_unix_ms,
                    "next_model_attempt": next_model_attempt,
                    "reason": reason
                }),
            ),
            LocalAgentStepOutcome::Pause { reason } => (
                LocalAgentRunStatus::Paused,
                None,
                None,
                None,
                "run_paused",
                json!({"reason": reason}),
            ),
            LocalAgentStepOutcome::NeedsReview { reason, detail } => (
                LocalAgentRunStatus::NeedsReview,
                None,
                None,
                None,
                "run_needs_review",
                json!({"reason": reason, "detail": detail}),
            ),
            LocalAgentStepOutcome::Succeed { output } => (
                LocalAgentRunStatus::Succeeded,
                None,
                None,
                Some(output.clone()),
                "run_succeeded",
                json!({"output": output}),
            ),
            LocalAgentStepOutcome::Fail { error, detail } => {
                let terminal = json!({"error": error, "detail": detail});
                (
                    LocalAgentRunStatus::Failed,
                    None,
                    None,
                    Some(terminal.clone()),
                    "run_failed",
                    terminal,
                )
            }
        };
    Ok(RunTransition {
        run_id: run.run_id.clone(),
        claim_token,
        expected_version,
        expected_status: LocalAgentRunStatus::ModelRunning,
        next_status,
        next_model_attempt,
        next_attempt_at_unix_ms: next_attempt,
        pending_tool_batch,
        tool_batch,
        checkpoint,
        clear_continuation_input,
        terminal_outcome,
        event_id: new_event_id(),
        event_type: event_type.to_string(),
        event_payload: payload,
        occurred_at_unix_ms: now,
    })
}
