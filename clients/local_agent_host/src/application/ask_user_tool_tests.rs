// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
    CreateRunCommand, HostCommand, HostRequestEnvelope, HostResult, ListEventsCommand,
    LocalAgentEventPayloadMode, LocalAgentRunStatus, LocalAgentStepOutcome, LocalAgentToolCall,
    ResumeRunCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::sync::Arc;

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

#[tokio::test]
async fn ask_user_tool_waits_and_resumes_with_the_original_tool_call() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime
        .try_handle(request(
            "create-ask-run",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-ask-user".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "task".to_string(),
                owner_entity_id: "task-1".to_string(),
                profile_key: "task_execution".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({
                    "source_conversation_id": "conversation-1",
                    "source_turn_id": "turn-1",
                    "prompt": "build"
                }),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("create run");
    let claim = runtime
        .try_handle(request(
            "claim-ask-run",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 30_000,
            }),
        ))
        .await
        .expect("claim run");
    let HostResult::Claim { claim: Some(claim) } = claim else {
        panic!("expected run claim");
    };
    runtime
        .try_handle(request(
            "ask-step",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "ask-batch".to_string(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: "ask-call-1".to_string(),
                        tool_name: ASK_USER_CHOICES_TOOL.to_string(),
                        arguments: json!({
                            "title": "Deployment",
                            "message": "Choose a target",
                            "options": [
                                {"value": "local", "label": "Local"},
                                {"value": "cloud", "label": "Cloud"}
                            ],
                            "default": "local"
                        }),
                        side_effecting: false,
                        requires_approval: false,
                    }],
                    checkpoint: json!({
                        "response": {
                            "request_input_items": [],
                            "response_output_items": [{
                                "type": "function_call",
                                "name": ASK_USER_CHOICES_TOOL,
                                "call_id": "ask-call-1",
                                "arguments": "{}"
                            }]
                        }
                    }),
                },
            }),
        ))
        .await
        .expect("wait for Ask User");
    let tool = runtime
        .try_handle(request(
            "claim-ask-tool",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "local-tool-worker".to_string(),
                lease_duration_ms: 30_000,
                include_tool_names: Some(vec![ASK_USER_CHOICES_TOOL.to_string()]),
                exclude_tool_names: Vec::new(),
            }),
        ))
        .await
        .expect("claim Ask User tool");
    let HostResult::ToolClaim { claim: Some(tool) } = tool else {
        panic!("expected Ask User tool claim");
    };
    let executor =
        LocalAskUserToolExecutor::new(Arc::clone(&runtime), "user-1").expect("Ask User executor");
    let outcome = executor
        .execute_tool(&tool.invocation)
        .await
        .expect("execute Ask User");
    let committed = runtime
        .try_handle(request(
            "commit-ask-tool",
            HostCommand::CommitTool(CommitToolCommand {
                owner_user_id: "user-1".to_string(),
                invocation_id: tool.invocation.invocation_id,
                claim_token: tool.claim_token,
                expected_version: tool.invocation.version,
                outcome,
            }),
        ))
        .await
        .expect("commit Ask User");
    let HostResult::ToolCommit { result } = committed else {
        panic!("expected tool commit");
    };
    assert_eq!(result.run.status, LocalAgentRunStatus::WaitingUser);
    assert!(result.run.pending_tool_batch.is_none());
    assert_eq!(
        result.run.checkpoint["response"]["response_output_items"][0]["call_id"],
        "ask-call-1"
    );

    let events = runtime
        .try_handle(request(
            "list-ask-events",
            HostCommand::ListEvents(ListEventsCommand {
                owner_user_id: "user-1".to_string(),
                after_cursor: 0,
                limit: 100,
                run_id: Some("run-ask-user".to_string()),
                event_type: Some("user_input_requested".to_string()),
                newest_first: false,
                payload_mode: LocalAgentEventPayloadMode::Full,
            }),
        ))
        .await
        .expect("list events");
    let HostResult::Events { events, .. } = events else {
        panic!("expected events");
    };
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["prompt"]["tool_call_id"], "ask-call-1");
    assert_eq!(events[0].payload["prompt"]["kind"], "choice");

    let resumed = runtime
        .try_handle(request(
            "resume-ask-run",
            HostCommand::ResumeRun(ResumeRunCommand {
                owner_user_id: "user-1".to_string(),
                run_id: "run-ask-user".to_string(),
                expected_version: result.run.version,
                expected_status: LocalAgentRunStatus::WaitingUser,
                reason: "ask_user_submitted".to_string(),
                input: json!({
                    "source": "ask_user",
                    "tool_call_id": "ask-call-1",
                    "values": {},
                    "selection": "local"
                }),
            }),
        ))
        .await
        .expect("resume Ask User");
    assert!(matches!(
        resumed,
        HostResult::Run { run }
            if run.status == LocalAgentRunStatus::ContinuationReady
                && run.continuation_input.as_ref().is_some_and(|value|
                    value["reason"] == "ask_user_submitted")
    ));
}

#[test]
fn ask_user_is_a_supported_task_capability_and_has_three_tools() {
    assert!(
        super::task_tool_support::validated_builtin_kinds(false, vec!["AskUser".to_string()])
            .is_ok()
    );
    assert_eq!(ask_user_model_tools().len(), 3);
}
