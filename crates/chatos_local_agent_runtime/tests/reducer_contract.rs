// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRun,
    LocalAgentRunStatus, ModelProtocol, ModelRuntimeDescriptor, ModelStepResult,
};
use chatos_local_agent_runtime::{
    reduce_claimed_event, HumanInteraction, ReducerError, ReducerPolicy, StepEvidence,
};
use chrono::{Duration, Utc};

fn model_descriptor() -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model-1".to_string(),
        revision: 1,
        provider: "openai".to_string(),
        model: "gpt-5".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    }
}

fn run(status: LocalAgentRunStatus) -> LocalAgentRun {
    let now = Utc::now();
    LocalAgentRun {
        run_id: "run-1".to_string(),
        profile_key: "task_runner".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "task".to_string(),
        owner_entity_id: "task-1".to_string(),
        project_id: Some("project-1".to_string()),
        status,
        version: 1,
        step_seq: 0,
        iteration: 0,
        retry_count: 0,
        model_config_id: "model-1".to_string(),
        model_config_revision: 1,
        model_runtime_snapshot: model_descriptor(),
        context_strategy: ContextStrategy::ProviderNative,
        prompt_revision: "prompt-1".to_string(),
        capability_snapshot_ref: "capabilities-1".to_string(),
        pending_batch_id: None,
        pending_interaction: None,
        terminal_outcome: None,
        deadline_at: None,
        created_at: now,
        updated_at: now,
    }
}

fn event(run: &LocalAgentRun, event_type: LocalAgentEventType) -> LocalAgentEvent {
    LocalAgentEvent {
        event_id: format!("event-{:?}", event_type),
        run_id: run.run_id.clone(),
        event_type,
        expected_version: run.version,
        available_at: Utc::now(),
        status: LocalAgentEventStatus::Claimed,
        attempt_count: 1,
        claimed_by_device_id: Some("device-1".to_string()),
        claim_token: Some("claim-1".to_string()),
        claim_until: Some(Utc::now() + Duration::seconds(30)),
        causation_id: "cause-1".to_string(),
        correlation_id: "turn-1".to_string(),
        bounded_payload: serde_json::Value::Null,
        last_error: None,
    }
}

#[test]
fn start_schedules_exactly_one_model_step() {
    let run = run(LocalAgentRunStatus::Queued);
    let reduction = reduce_claimed_event(
        &run,
        &event(&run, LocalAgentEventType::RunStarted),
        StepEvidence::None,
        Utc::now(),
        ReducerPolicy::default(),
    )
    .unwrap();
    assert_eq!(reduction.run.status, LocalAgentRunStatus::ModelReady);
    assert_eq!(reduction.run.version, 2);
    assert_eq!(reduction.emitted_events.len(), 1);
    assert_eq!(
        reduction.emitted_events[0].event_type,
        LocalAgentEventType::ModelStepRequested
    );
    assert_eq!(reduction.emitted_events[0].expected_version, 2);
}

#[test]
fn tool_commands_freeze_a_batch_before_execution() {
    let run = run(LocalAgentRunStatus::ModelRunning);
    let reduction = reduce_claimed_event(
        &run,
        &event(&run, LocalAgentEventType::ModelStepCompleted),
        StepEvidence::Model {
            result: ModelStepResult::ToolCommand(serde_json::json!({"calls": ["call-1"]})),
            pending_batch_id: Some("batch-1".to_string()),
            retry_at: None,
        },
        Utc::now(),
        ReducerPolicy::default(),
    )
    .unwrap();
    assert_eq!(reduction.run.status, LocalAgentRunStatus::WaitingToolResult);
    assert_eq!(reduction.run.pending_batch_id.as_deref(), Some("batch-1"));
    assert_eq!(
        reduction.emitted_events[0].event_type,
        LocalAgentEventType::ToolBatchRequested
    );
}

#[test]
fn completed_local_execution_emits_a_durable_batch_completion() {
    let mut run = run(LocalAgentRunStatus::WaitingToolResult);
    run.pending_batch_id = Some("batch-1".to_string());
    let reduction = reduce_claimed_event(
        &run,
        &event(&run, LocalAgentEventType::ToolBatchRequested),
        StepEvidence::ToolBatch {
            outcome_unknown: false,
        },
        Utc::now(),
        ReducerPolicy::default(),
    )
    .unwrap();
    assert_eq!(reduction.run.status, LocalAgentRunStatus::WaitingToolResult);
    assert_eq!(reduction.emitted_events.len(), 1);
    assert_eq!(
        reduction.emitted_events[0].event_type,
        LocalAgentEventType::ToolBatchCompleted
    );
    assert_eq!(
        reduction.emitted_events[0].bounded_payload["outcome_unknown"],
        false
    );
}

#[test]
fn unknown_tool_outcome_requires_human_review_and_is_not_replayed() {
    let mut run = run(LocalAgentRunStatus::WaitingToolResult);
    run.pending_batch_id = Some("batch-1".to_string());
    let reduction = reduce_claimed_event(
        &run,
        &event(&run, LocalAgentEventType::ToolBatchCompleted),
        StepEvidence::ToolBatch {
            outcome_unknown: true,
        },
        Utc::now(),
        ReducerPolicy::default(),
    )
    .unwrap();
    assert_eq!(reduction.run.status, LocalAgentRunStatus::NeedsReview);
    assert!(reduction.emitted_events.is_empty());
    assert_eq!(
        reduction.human_interaction,
        Some(HumanInteraction::ReviewUnknownToolOutcome {
            batch_id: "batch-1".to_string(),
        })
    );
}

#[test]
fn ask_user_preserves_the_visual_question_for_durable_storage() {
    let run = run(LocalAgentRunStatus::ModelRunning);
    let question = serde_json::json!({
        "prompt": "Choose a visual direction",
        "options": [],
        "image_references": ["preview-1"],
        "details": null
    });
    let reduction = reduce_claimed_event(
        &run,
        &event(&run, LocalAgentEventType::ModelStepCompleted),
        StepEvidence::Model {
            result: ModelStepResult::AskUser(question.clone()),
            pending_batch_id: None,
            retry_at: None,
        },
        Utc::now(),
        ReducerPolicy::default(),
    )
    .unwrap();
    assert_eq!(reduction.run.status, LocalAgentRunStatus::Paused);
    assert_eq!(
        reduction.human_interaction,
        Some(HumanInteraction::AskUser(question))
    );
}

#[test]
fn retries_are_bounded_and_then_fail_terminally() {
    let mut run = run(LocalAgentRunStatus::ModelRunning);
    run.retry_count = 1;
    let now = Utc::now();
    let reduction = reduce_claimed_event(
        &run,
        &event(&run, LocalAgentEventType::ModelStepCompleted),
        StepEvidence::Model {
            result: ModelStepResult::Retry(serde_json::json!({"error": "timeout"})),
            pending_batch_id: None,
            retry_at: Some(now + Duration::seconds(5)),
        },
        now,
        ReducerPolicy { max_retries: 1 },
    )
    .unwrap();
    assert_eq!(reduction.run.status, LocalAgentRunStatus::Failed);
    assert_eq!(
        reduction.emitted_events[0].event_type,
        LocalAgentEventType::RunTerminal
    );
}

#[test]
fn stale_events_cannot_advance_a_run() {
    let run = run(LocalAgentRunStatus::Queued);
    let mut stale = event(&run, LocalAgentEventType::RunStarted);
    stale.expected_version = 2;
    assert_eq!(
        reduce_claimed_event(
            &run,
            &stale,
            StepEvidence::None,
            Utc::now(),
            ReducerPolicy::default(),
        ),
        Err(ReducerError::VersionConflict {
            expected: 2,
            actual: 1,
        })
    );
}
