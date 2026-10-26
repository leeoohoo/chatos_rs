// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::path::PathBuf;

fn shared_fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../contracts/local-agent/v48/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read shared contract fixture {}: {error}",
            path.display()
        )
    })
}

#[test]
fn shared_health_fixtures_match_protocol_v48() {
    let request: HostRequestEnvelope =
        serde_json::from_slice(&shared_fixture("health-request.json")).expect("health request");
    request.validate().expect("valid health request");
    assert_eq!(request.protocol_version, LOCAL_AGENT_PROTOCOL_VERSION);
    assert_eq!(request.command_id, "contract-health-1");
    assert_eq!(request.command, HostCommand::Health);

    let success: HostResponseEnvelope =
        serde_json::from_slice(&shared_fixture("health-success.json")).expect("health success");
    assert!(success.ok);
    assert!(success.error.is_none());
    assert!(matches!(
        success.result,
        Some(HostResult::Health {
            service,
            storage_ready: true,
            recovered_claims: 0,
            max_frame_bytes: LOCAL_AGENT_MAX_FRAME_BYTES,
            max_message_bytes: LOCAL_AGENT_MAX_MESSAGE_BYTES,
            scheduler: Some(LocalAgentHostSchedulerSnapshot {
                maximum_concurrent_work_items: 4,
                effective_maximum_concurrent_work_items: 3,
                pressure_level: LocalAgentResourcePressureLevel::Elevated,
                active_work_items: 2,
                idle_workers: 2,
                queued_ready_work_items: 3,
            }),
        }) if service == "chatos-local-agent-host"
    ));

    let failure: HostResponseEnvelope =
        serde_json::from_slice(&shared_fixture("health-failure.json")).expect("health failure");
    assert!(!failure.ok);
    assert!(failure.result.is_none());
    assert_eq!(
        failure.error.expect("health error").code,
        "storage_unavailable"
    );
}

#[test]
fn shared_conversation_run_fixtures_preserve_cross_platform_links() {
    let history: HostResponseEnvelope =
        serde_json::from_slice(&shared_fixture("conversation-history-success.json"))
            .expect("conversation history");
    let history = match history.result.expect("history result") {
        HostResult::ConversationHistory { page } => page,
        other => panic!("unexpected history result: {other:?}"),
    };
    assert_eq!(history.conversation.conversation_id, "conversation-1");
    assert_eq!(history.turns[0].run_id, "run-1");
    assert_eq!(history.messages[0].message_id, "message-user-1");
    assert_eq!(
        history.attachments[0].authorized_local_ref,
        "local-attachment:authority-1"
    );
    assert_eq!(history.next_before_ordinal, None);

    let graph: HostResponseEnvelope =
        serde_json::from_slice(&shared_fixture("message-task-graph-success.json"))
            .expect("message task graph");
    let graph = match graph.result.expect("graph result") {
        HostResult::MessageTaskGraph { graph } => graph,
        other => panic!("unexpected graph result: {other:?}"),
    };
    assert_eq!(
        graph.source_conversation_id,
        history.conversation.conversation_id
    );
    assert_eq!(graph.source_turn_id, history.turns[0].turn_id);
    assert_eq!(
        graph.source_user_message_id.as_deref(),
        Some(history.messages[0].message_id.as_str())
    );
    assert_eq!(graph.nodes[0].task.active_run_id.as_deref(), Some("run-1"));

    let events: HostResponseEnvelope =
        serde_json::from_slice(&shared_fixture("run-events-latest-success.json"))
            .expect("latest run events");
    let (events, next_cursor) = match events.result.expect("events result") {
        HostResult::Events {
            events,
            next_cursor,
        } => (events, next_cursor),
        other => panic!("unexpected events result: {other:?}"),
    };
    assert!(events
        .windows(2)
        .all(|pair| pair[0].cursor > pair[1].cursor));
    assert!(events.iter().all(|event| event.run_id == "run-1"));
    assert_eq!(next_cursor, events.last().expect("oldest event").cursor);
}

#[test]
fn run_status_round_trips_through_stable_wire_value() {
    for status in [
        LocalAgentRunStatus::Queued,
        LocalAgentRunStatus::ModelReady,
        LocalAgentRunStatus::ModelRunning,
        LocalAgentRunStatus::WaitingToolResult,
        LocalAgentRunStatus::ContinuationReady,
        LocalAgentRunStatus::WaitingUser,
        LocalAgentRunStatus::RetryScheduled,
        LocalAgentRunStatus::Paused,
        LocalAgentRunStatus::NeedsReview,
        LocalAgentRunStatus::Succeeded,
        LocalAgentRunStatus::Failed,
        LocalAgentRunStatus::Cancelled,
    ] {
        assert_eq!(LocalAgentRunStatus::from_str(status.as_str()), Ok(status));
    }
}

#[test]
fn request_rejects_zero_version_and_unbounded_event_page() {
    let commit = CommitStepCommand {
        owner_user_id: "user-1".to_string(),
        run_id: "run-1".to_string(),
        claim_token: "claim-1".to_string(),
        expected_version: 0,
        outcome: LocalAgentStepOutcome::Succeed {
            output: Value::Null,
        },
    };
    assert!(commit.validate().is_err());
    assert!(ListEventsCommand {
        owner_user_id: "user-1".to_string(),
        after_cursor: 0,
        limit: LOCAL_AGENT_MAX_EVENT_PAGE_SIZE + 1,
        run_id: None,
        event_type: None,
        newest_first: false,
        payload_mode: LocalAgentEventPayloadMode::Full,
    }
    .validate()
    .is_err());
    assert!(WaitEventsCommand {
        owner_user_id: "user-1".to_string(),
        after_cursor: 0,
        limit: 10,
        run_id: None,
        timeout_ms: 60_001,
        payload_mode: LocalAgentEventPayloadMode::Full,
    }
    .validate()
    .is_err());
    let latest = ListEventsCommand {
        owner_user_id: "user-1".to_string(),
        after_cursor: 0,
        limit: 1,
        run_id: Some("run-1".to_string()),
        event_type: Some("user_input_requested".to_string()),
        newest_first: true,
        payload_mode: LocalAgentEventPayloadMode::Full,
    };
    assert!(latest.validate().is_ok());
    assert!(ListEventsCommand {
        limit: LOCAL_AGENT_MAX_EVENT_PAGE_SIZE,
        ..latest.clone()
    }
    .validate()
    .is_ok());
    assert!(ListEventsCommand {
        run_id: None,
        newest_first: false,
        ..latest
    }
    .validate()
    .is_err());
}

#[test]
fn event_payload_mode_defaults_to_full_and_uses_stable_wire_values() {
    let command: HostCommand = serde_json::from_value(serde_json::json!({
        "type": "list_events",
        "owner_user_id": "user-1",
        "after_cursor": 0,
        "limit": 100,
        "run_id": null
    }))
    .expect("legacy list events command");
    assert!(matches!(
        command,
        HostCommand::ListEvents(ListEventsCommand {
            payload_mode: LocalAgentEventPayloadMode::Full,
            ..
        })
    ));
    assert_eq!(
        serde_json::to_value(LocalAgentEventPayloadMode::Process).expect("process mode"),
        serde_json::json!("process")
    );
    assert_eq!(
        serde_json::to_value(LocalAgentEventPayloadMode::Routing).expect("routing mode"),
        serde_json::json!("routing")
    );
    assert_eq!(
        serde_json::to_value(LocalAgentEventPayloadMode::None).expect("none mode"),
        serde_json::json!("none")
    );
}
