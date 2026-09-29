// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CancelRunCommand, CreateRunCommand, GetRunCommand, HostCommand, HostRequestEnvelope,
    HostResult, ListEventsCommand, ResumeRunCommand, WaitEventsCommand,
    LOCAL_AGENT_PROTOCOL_VERSION,
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

fn create(run_id: &str, owner_user_id: &str) -> HostCommand {
    HostCommand::CreateRun(CreateRunCommand {
        run_id: run_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        owner_entity_type: "test".to_string(),
        owner_entity_id: format!("entity-{run_id}"),
        profile_key: "main_chat".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({"message": run_id}),
        max_iterations: 4,
    })
}

#[tokio::test]
async fn run_detail_mutations_and_events_are_owner_scoped() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(20_000)));
    runtime
        .try_handle(request("create-a", create("run-a", "user-1")))
        .await
        .expect("create user-1 Run");
    runtime
        .try_handle(request("create-b", create("run-b", "user-2")))
        .await
        .expect("create user-2 Run");

    assert!(runtime
        .try_handle(request(
            "cross-get",
            HostCommand::GetRun(GetRunCommand {
                owner_user_id: "user-2".to_string(),
                run_id: "run-a".to_string(),
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-resume",
            HostCommand::ResumeRun(ResumeRunCommand {
                owner_user_id: "user-2".to_string(),
                run_id: "run-a".to_string(),
                expected_version: 1,
                expected_status: LocalAgentRunStatus::Paused,
                reason: "resume from inspector".to_string(),
                input: json!({}),
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-cancel",
            HostCommand::CancelRun(CancelRunCommand {
                owner_user_id: "user-2".to_string(),
                run_id: "run-a".to_string(),
                expected_version: Some(1),
                reason: "cancel from inspector".to_string(),
            }),
        ))
        .await
        .is_err());

    runtime
        .try_handle(request(
            "cancel-a",
            HostCommand::CancelRun(CancelRunCommand {
                owner_user_id: "user-1".to_string(),
                run_id: "run-a".to_string(),
                expected_version: Some(1),
                reason: "cancel from inspector".to_string(),
            }),
        ))
        .await
        .expect("owner cancels Run");

    let user_one_events = runtime
        .try_handle(request(
            "events-a",
            HostCommand::ListEvents(ListEventsCommand {
                owner_user_id: "user-1".to_string(),
                after_cursor: 0,
                limit: 100,
                run_id: None,
            }),
        ))
        .await
        .expect("list user-1 events");
    assert!(matches!(
        user_one_events,
        HostResult::Events { events, .. }
            if events.len() == 2 && events.iter().all(|event| event.run_id == "run-a")
    ));

    let cross_filtered = runtime
        .try_handle(request(
            "wait-cross-filter",
            HostCommand::WaitEvents(WaitEventsCommand {
                owner_user_id: "user-2".to_string(),
                after_cursor: 0,
                limit: 100,
                run_id: Some("run-a".to_string()),
                timeout_ms: 1,
            }),
        ))
        .await
        .expect("wait cross-owner filter");
    assert!(matches!(
        cross_filtered,
        HostResult::Events { events, next_cursor: 0 } if events.is_empty()
    ));
}
