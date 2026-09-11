// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, ContextStrategy, LocalAgentCommand, LocalAgentEvent,
    LocalAgentEventStatus, LocalAgentEventType, LocalAgentIpcRequest, LocalAgentRun,
    LocalAgentRunStatus, MemorySyncStatus, MessageMode, ProtocolError, ToolEffect, ToolExecution,
    ToolExecutionStatus, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chrono::Utc;

fn run(status: LocalAgentRunStatus) -> LocalAgentRun {
    let now = Utc::now();
    LocalAgentRun {
        run_id: "run-1".to_string(),
        profile_key: "main_chat".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "conversation-1".to_string(),
        project_id: None,
        status,
        version: 1,
        step_seq: 0,
        iteration: 0,
        retry_count: 0,
        model_config_id: "model-1".to_string(),
        model_config_revision: 1,
        model_runtime_snapshot: serde_json::json!({"provider": "openai"}),
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

#[test]
fn terminal_runs_require_a_terminal_outcome() {
    assert!(run(LocalAgentRunStatus::Queued).validate().is_ok());
    assert!(matches!(
        run(LocalAgentRunStatus::Succeeded).validate(),
        Err(ProtocolError::InvalidState { .. })
    ));

    let mut complete = run(LocalAgentRunStatus::Succeeded);
    complete.terminal_outcome = Some(serde_json::json!({"result": "done"}));
    assert!(complete.validate().is_ok());
}

#[test]
fn waiting_tool_runs_require_a_batch_identity() {
    assert!(matches!(
        run(LocalAgentRunStatus::WaitingToolResult).validate(),
        Err(ProtocolError::EmptyIdentifier {
            field: "pending_batch_id"
        })
    ));
}

#[test]
fn events_reject_unbounded_payloads() {
    let event = LocalAgentEvent {
        event_id: "event-1".to_string(),
        run_id: "run-1".to_string(),
        event_type: LocalAgentEventType::ModelStepCompleted,
        expected_version: 1,
        available_at: Utc::now(),
        status: LocalAgentEventStatus::Pending,
        attempt_count: 0,
        claimed_by_device_id: None,
        claim_token: None,
        claim_until: None,
        causation_id: "event-0".to_string(),
        correlation_id: "turn-1".to_string(),
        bounded_payload: serde_json::json!({"value": "x".repeat(70_000)}),
        last_error: None,
    };
    assert!(matches!(
        event.validate(),
        Err(ProtocolError::PayloadTooLarge { .. })
    ));
}

#[test]
fn tool_results_preserve_unknown_outcomes_without_marking_completion() {
    let execution = ToolExecution {
        invocation_id: "invocation-1".to_string(),
        run_id: "run-1".to_string(),
        batch_id: "batch-1".to_string(),
        tool_call_id: "call-1".to_string(),
        tool_name: "write_file".to_string(),
        effect: ToolEffect::Write,
        arguments_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        status: ToolExecutionStatus::OutcomeUnknown,
        bounded_result: None,
        started_at: Some(Utc::now()),
        completed_at: None,
    };
    assert!(execution.validate().is_ok());
    assert!(execution.effect.requires_durable_start());
}

#[test]
fn tool_messages_require_the_provider_call_identity() {
    let message = AgentMessage {
        record_id: "message-1".to_string(),
        run_id: "run-1".to_string(),
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        sequence: 1,
        role: AgentMessageRole::Tool,
        content: Some("done".to_string()),
        reasoning: None,
        structured_payload: None,
        tool_call_id: None,
        response_id: None,
        message_mode: MessageMode::Semantic,
        message_source: "local_tool_runtime".to_string(),
        memory_sync_status: MemorySyncStatus::Pending,
        created_at: Utc::now(),
    };
    assert!(matches!(
        message.validate(),
        Err(ProtocolError::EmptyIdentifier {
            field: "tool_call_id"
        })
    ));
}

#[test]
fn ipc_request_validates_the_nested_command() {
    let request = LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "request-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::ListRuns {
            cursor: None,
            limit: 0,
        },
    };
    assert!(matches!(
        request.validate(),
        Err(ProtocolError::InvalidState { .. })
    ));
}

#[test]
fn event_claim_lease_fields_are_all_or_nothing() {
    let mut event = LocalAgentEvent {
        event_id: "event-1".to_string(),
        run_id: "run-1".to_string(),
        event_type: LocalAgentEventType::RunStarted,
        expected_version: 1,
        available_at: Utc::now(),
        status: LocalAgentEventStatus::Pending,
        attempt_count: 0,
        claimed_by_device_id: None,
        claim_token: Some("orphan-token".to_string()),
        claim_until: None,
        causation_id: "turn-1".to_string(),
        correlation_id: "run-1".to_string(),
        bounded_payload: serde_json::Value::Null,
        last_error: None,
    };
    assert!(matches!(
        event.validate(),
        Err(ProtocolError::InvalidState { .. })
    ));
    event.status = LocalAgentEventStatus::Claimed;
    event.claimed_by_device_id = Some("device-1".to_string());
    event.claim_until = Some(Utc::now());
    assert!(event.validate().is_ok());
}
