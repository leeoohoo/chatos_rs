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
    // A continuation is consumed only after the model produced a new durable result.
    // Pre-request validation/provider failures and pauses do not prove that tool results
    // reached the provider, so keep them available for a retry or human resume.
    let clear_continuation_input = matches!(
        &outcome,
        LocalAgentStepOutcome::Continue { .. }
            | LocalAgentStepOutcome::WaitForTool { .. }
            | LocalAgentStepOutcome::WaitForUser { .. }
            | LocalAgentStepOutcome::Succeed { .. }
    );
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
            LocalAgentStepOutcome::NeedsReview { reason, detail } => {
                let error = detail
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|error| !error.is_empty())
                    .unwrap_or(&reason);
                let terminal = json!({
                    "status": "needs_review",
                    "reason_code": "step_requires_review",
                    "error": error,
                    "reason": reason,
                    "detail": detail,
                });
                (
                    LocalAgentRunStatus::NeedsReview,
                    None,
                    None,
                    Some(terminal),
                    "run_needs_review",
                    json!({"reason": reason, "detail": detail}),
                )
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn run_with_tool_results() -> LocalAgentRunRecord {
        LocalAgentRunRecord {
            run_id: "run-1".into(),
            owner_user_id: "user-1".into(),
            owner_entity_type: "conversation_turn".into(),
            owner_entity_id: "turn-1".into(),
            profile_key: "main_chat".into(),
            model_config_ref: "model-1".into(),
            model_config_revision: "revision-1".into(),
            capability_policy_revision: "policy-1".into(),
            input: json!({"message":"hello"}),
            status: LocalAgentRunStatus::ModelRunning,
            iteration: 2,
            model_attempt: 1,
            max_iterations: 8,
            version: 4,
            claim_token: Some("claim-1".into()),
            claim_until_unix_ms: Some(20_000),
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: json!({"response":{"response_output_items":[{
                "type":"function_call", "call_id":"call-1", "name":"write_file"
            }]}}),
            continuation_input: Some(json!({
                "type":"tool_results", "invocations":[{"call_id":"call-1"}]
            })),
            terminal_outcome: None,
            created_at_unix_ms: 1_000,
            updated_at_unix_ms: 2_000,
        }
    }

    #[test]
    fn failures_pauses_and_retries_keep_unconsumed_tool_results() {
        let run = run_with_tool_results();
        let outcomes = [
            LocalAgentStepOutcome::Retry {
                resume_at_unix_ms: 4_000,
                next_model_attempt: 2,
                reason: "provider busy".into(),
            },
            LocalAgentStepOutcome::Pause {
                reason: "cancelled before commit".into(),
            },
            LocalAgentStepOutcome::NeedsReview {
                reason: "profile step failed".into(),
                detail: json!({"error":"request exceeds budget"}),
            },
            LocalAgentStepOutcome::Fail {
                error: "No tool output found for function call call-1".into(),
                detail: json!({"phase":"model_step"}),
            },
        ];
        for outcome in outcomes {
            let transition =
                transition_for_outcome(&run, "claim-1".into(), run.version, outcome, 3_000)
                    .expect("transition");
            assert!(!transition.clear_continuation_input);
        }
    }

    #[test]
    fn completed_model_response_consumes_tool_results() {
        let run = run_with_tool_results();
        let transition = transition_for_outcome(
            &run,
            "claim-1".into(),
            run.version,
            LocalAgentStepOutcome::Succeed {
                output: Value::String("done".into()),
            },
            3_000,
        )
        .expect("transition");
        assert!(transition.clear_continuation_input);
    }
}
