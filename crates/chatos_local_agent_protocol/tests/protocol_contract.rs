// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, ContextStrategy, LocalAgentCommand, LocalAgentEvent,
    LocalAgentEventStatus, LocalAgentEventType, LocalAgentIpcRequest, LocalAgentRun,
    LocalAgentRunStatus, MemorySyncStatus, MessageMode, ModelGatewayParameters,
    ModelGatewayRequest, ModelGatewayStreamEnvelope, ModelGatewayStreamEvent, ModelGatewayTerminal,
    ModelGatewayTerminalSource, ModelGatewayTerminalStatus, ModelGatewayTokenCount, ModelProtocol,
    ModelRuntimeDescriptor, ModelStepCompletion, ModelStepResult, ProtocolError,
    ProviderContextItem, ToolEffect, ToolExecution, ToolExecutionStatus,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chrono::Utc;

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
fn run_rejects_a_descriptor_from_another_revision() {
    let mut run = run(LocalAgentRunStatus::Queued);
    run.model_runtime_snapshot.revision = 2;
    assert!(matches!(
        run.validate(),
        Err(ProtocolError::InvalidState { .. })
    ));
}

#[test]
fn gateway_request_contains_no_provider_secret_or_endpoint() {
    let descriptor = model_descriptor();
    let request = ModelGatewayRequest {
        request_id: "request-1".to_string(),
        model_config_id: descriptor.model_config_id.clone(),
        model_config_revision: descriptor.revision,
        protocol: descriptor.protocol,
        input: serde_json::json!([{"role": "user", "content": "hello"}]),
        tools: Vec::new(),
        instructions: Some("Answer clearly".to_string()),
        parameters: ModelGatewayParameters {
            maximum_output_tokens: 4_096,
            reasoning_effort: Some("high".to_string()),
            temperature: None,
            native_compaction_threshold: Some(200_000),
        },
    };
    request.validate_against(&descriptor).unwrap();
    let encoded = serde_json::to_string(&request).unwrap();
    assert!(!encoded.contains("api_key"));
    assert!(!encoded.contains("base_url"));

    let mut stale = request;
    stale.model_config_revision += 1;
    assert!(matches!(
        stale.validate_against(&descriptor),
        Err(ProtocolError::InvalidState { .. })
    ));
}

#[test]
fn gateway_request_requires_exactly_one_configured_context_strategy() {
    let descriptor = model_descriptor();
    let mut request = ModelGatewayRequest {
        request_id: "request-1".to_string(),
        model_config_id: descriptor.model_config_id.clone(),
        model_config_revision: descriptor.revision,
        protocol: descriptor.protocol,
        input: serde_json::json!([{"role": "user", "content": "hello"}]),
        tools: Vec::new(),
        instructions: None,
        parameters: ModelGatewayParameters {
            maximum_output_tokens: 32_000,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: None,
        },
    };
    assert!(matches!(
        request.validate_against(&descriptor),
        Err(ProtocolError::InvalidState { .. })
    ));

    request.parameters.native_compaction_threshold = Some(368_001);
    assert!(matches!(
        request.validate_against(&descriptor),
        Err(ProtocolError::InvalidState { .. })
    ));

    let mut memory_descriptor = descriptor;
    memory_descriptor.context_strategy = ContextStrategy::MemoryEngine;
    memory_descriptor.supports_native_compaction = false;
    request.parameters.native_compaction_threshold = Some(200_000);
    assert!(matches!(
        request.validate_against(&memory_descriptor),
        Err(ProtocolError::InvalidState { .. })
    ));
    request.parameters.native_compaction_threshold = None;
    request.validate_against(&memory_descriptor).unwrap();
}

#[test]
fn token_count_response_is_bound_to_the_exact_gateway_request() {
    let descriptor = model_descriptor();
    let request = ModelGatewayRequest {
        request_id: "request-1".to_string(),
        model_config_id: descriptor.model_config_id,
        model_config_revision: descriptor.revision,
        protocol: descriptor.protocol,
        input: serde_json::json!([{"role": "user", "content": "hello"}]),
        tools: Vec::new(),
        instructions: None,
        parameters: ModelGatewayParameters {
            maximum_output_tokens: 4_096,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: Some(200_000),
        },
    };
    let mut count = ModelGatewayTokenCount {
        request_id: request.request_id.clone(),
        model_config_id: request.model_config_id.clone(),
        model_config_revision: request.model_config_revision,
        input_tokens: 42,
    };
    count.validate_against(&request).unwrap();

    count.request_id = "another-request".to_string();
    assert!(matches!(
        count.validate_against(&request),
        Err(ProtocolError::InvalidState { .. })
    ));
}

#[test]
fn gateway_terminal_keeps_official_status_usage_and_request_identity() {
    let terminal = ModelGatewayTerminal {
        status: ModelGatewayTerminalStatus::Incomplete,
        source: ModelGatewayTerminalSource::Provider,
        response_id: Some("resp_1".to_string()),
        provider_request_id: Some("req_1".to_string()),
        terminal_event: "response.incomplete".to_string(),
        provider_http_status: Some(200),
        usage: Some(serde_json::json!({"input_tokens": 100, "output_tokens": 20})),
        output_items: vec![serde_json::json!({"type": "message", "id": "msg_1"})],
        incomplete_details: Some(serde_json::json!({"reason": "max_output_tokens"})),
        provider_error: None,
    };
    let envelope = ModelGatewayStreamEnvelope {
        request_id: "request-1".to_string(),
        sequence: 3,
        protocol: ModelProtocol::Responses,
        event: ModelGatewayStreamEvent::Terminal {
            terminal: Box::new(terminal),
        },
    };
    envelope.validate().unwrap();

    let mut invalid = envelope;
    let ModelGatewayStreamEvent::Terminal { terminal } = &mut invalid.event else {
        unreachable!();
    };
    terminal.incomplete_details = None;
    assert!(matches!(
        invalid.validate(),
        Err(ProtocolError::InvalidState { .. })
    ));
}

#[test]
fn durable_model_completion_metadata_must_match_its_result() {
    let valid = ModelStepCompletion {
        result: ModelStepResult::Retry(serde_json::json!({"reason": "overloaded"})),
        pending_batch_id: None,
        retry_at: Some(Utc::now()),
    };
    valid.validate().unwrap();

    let invalid = ModelStepCompletion {
        result: ModelStepResult::Final(serde_json::json!({"text": "done"})),
        pending_batch_id: Some("batch-1".to_string()),
        retry_at: None,
    };
    assert!(matches!(
        invalid.validate(),
        Err(ProtocolError::InvalidState { .. })
    ));
}

#[test]
fn gateway_failures_cannot_impersonate_provider_terminals() {
    let gateway_failure = ModelGatewayTerminal {
        status: ModelGatewayTerminalStatus::Failed,
        source: ModelGatewayTerminalSource::Gateway,
        response_id: None,
        provider_request_id: None,
        terminal_event: "gateway.provider_request_failed".to_string(),
        provider_http_status: None,
        usage: None,
        output_items: Vec::new(),
        incomplete_details: None,
        provider_error: Some(serde_json::json!({"message": "connection failed"})),
    };
    gateway_failure.validate(ModelProtocol::Responses).unwrap();

    let mut invented_response = gateway_failure.clone();
    invented_response.response_id = Some("resp_fake".to_string());
    assert!(matches!(
        invented_response.validate(ModelProtocol::Responses),
        Err(ProtocolError::InvalidState { .. })
    ));

    let mut false_success = gateway_failure;
    false_success.status = ModelGatewayTerminalStatus::Completed;
    false_success.provider_error = None;
    assert!(matches!(
        false_success.validate(ModelProtocol::Responses),
        Err(ProtocolError::InvalidState { .. })
    ));
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
fn provider_context_payload_is_not_limited_to_identifier_length() {
    let item = ProviderContextItem {
        item_id: "context-1".to_string(),
        run_id: "run-1".to_string(),
        generation: 1,
        sequence: 1,
        provider: "openai".to_string(),
        item_type: "response_item".to_string(),
        encrypted_payload: "x".repeat(4 * 1024),
        payload_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        created_at: Utc::now(),
    };

    item.validate().unwrap();
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
