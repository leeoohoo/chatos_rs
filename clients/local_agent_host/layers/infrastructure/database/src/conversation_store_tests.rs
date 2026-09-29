// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_local_agent_ports::LocalAgentRunStore;
use chatos_local_agent_protocol::{LocalAgentRunStatus, LocalConversationAttachmentSpec};
use serde_json::{json, Value};

fn idempotency(command_id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: command_id.to_string(),
        request_fingerprint: command_id.to_string(),
    }
}

fn conversation(conversation_id: &str) -> CreateConversationCommand {
    CreateConversationCommand {
        conversation_id: conversation_id.to_string(),
        owner_user_id: "user-1".to_string(),
        title: "Local conversation".to_string(),
    }
}

fn turn(
    conversation_id: &str,
    expected_conversation_version: u64,
    suffix: &str,
) -> StartConversationTurnCommand {
    StartConversationTurnCommand {
        conversation_id: conversation_id.to_string(),
        expected_conversation_version,
        turn_id: format!("turn-{suffix}"),
        message_id: format!("message-{suffix}"),
        run_id: format!("run-{suffix}"),
        message: format!("hello {suffix}"),
        message_metadata: json!({"source": "test"}),
        attachments: Vec::new(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        max_iterations: 8,
    }
}

fn run(turn: &StartConversationTurnCommand, now: i64) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: turn.run_id.clone(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation_turn".to_string(),
        owner_entity_id: turn.turn_id.clone(),
        profile_key: "main_chat".to_string(),
        model_config_ref: turn.model_config_ref.clone(),
        model_config_revision: turn.model_config_revision.clone(),
        capability_policy_revision: turn.capability_policy_revision.clone(),
        input: json!({"message": turn.message}),
        status: LocalAgentRunStatus::Queued,
        iteration: 0,
        model_attempt: 1,
        max_iterations: turn.max_iterations,
        version: 1,
        claim_token: None,
        claim_until_unix_ms: None,
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        checkpoint: Value::Null,
        continuation_input: None,
        terminal_outcome: None,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    }
}

async fn create(storage: &SqliteClientStorage, conversation_id: &str) -> LocalConversationDetail {
    storage
        .create_conversation(
            &idempotency(&format!("create-{conversation_id}")),
            &conversation(conversation_id),
            1_000,
        )
        .await
        .expect("create conversation")
}

async fn start(
    storage: &SqliteClientStorage,
    turn: &StartConversationTurnCommand,
    now: i64,
) -> Result<LocalConversationTurnStart, ClientStorageError> {
    storage
        .start_conversation_turn(
            &idempotency(&format!("start-{}", turn.turn_id)),
            turn,
            &run(turn, now),
            &format!("event-{}", turn.turn_id),
            now,
        )
        .await
}

#[tokio::test]
async fn turn_start_is_atomic_and_rejects_stale_or_concurrent_writes() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create(&storage, "conversation-1").await;

    let stale = turn("conversation-1", 2, "stale");
    assert!(matches!(
        start(&storage, &stale, 2_000).await,
        Err(ClientStorageError::Conflict(_))
    ));
    assert!(storage
        .get_run(&stale.run_id)
        .await
        .expect("get stale Run")
        .is_none());
    let after_stale = storage
        .get_conversation("conversation-1")
        .await
        .expect("get conversation")
        .expect("conversation");
    assert!(after_stale.turns.is_empty());
    assert!(after_stale.messages.is_empty());

    let mut first = turn("conversation-1", 1, "first");
    first.attachments.push(LocalConversationAttachmentSpec {
        attachment_id: "attachment-first".to_string(),
        display_name: "brief.pdf".to_string(),
        media_type: "application/pdf".to_string(),
        byte_size: 42,
        sha256: "a".repeat(64),
        authorized_local_ref: "local-attachment:authority-first".to_string(),
        metadata: json!({"page_count": 1}),
    });
    let started = start(&storage, &first, 3_000).await.expect("start first");
    assert_eq!(started.conversation.version, 2);
    assert_eq!(started.turn.status, LocalConversationTurnStatus::Running);
    assert_eq!(started.message.ordinal, 1);
    assert_eq!(started.attachments.len(), 1);
    assert_eq!(started.attachments[0].ordinal, 1);
    assert_eq!(started.attachments[0].byte_size, 42);
    assert_eq!(started.run.status, LocalAgentRunStatus::Queued);

    let concurrent = turn("conversation-1", 2, "concurrent");
    assert!(matches!(
        start(&storage, &concurrent, 4_000).await,
        Err(ClientStorageError::Conflict(_))
    ));
    assert!(storage
        .get_run(&concurrent.run_id)
        .await
        .expect("get concurrent Run")
        .is_none());
}

#[tokio::test]
async fn terminal_runs_reconcile_turns_and_assistant_messages() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create(&storage, "conversation-success").await;
    let success_turn = turn("conversation-success", 1, "success");
    start(&storage, &success_turn, 2_000)
        .await
        .expect("start success Turn");
    let claim = storage
        .claim_next_run(
            &idempotency("claim-success"),
            "worker-1",
            "token-success",
            3_000,
            13_000,
            "event-claim-success",
        )
        .await
        .expect("claim")
        .expect("claimed Run");
    storage
        .apply_transition(
            &idempotency("complete-success"),
            &chatos_local_agent_ports::RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::Succeeded,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({"answer": "done"})),
                event_id: "event-success".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({"answer": "done"}),
                occurred_at_unix_ms: 4_000,
            },
        )
        .await
        .expect("complete Run");
    let succeeded = storage
        .get_conversation("conversation-success")
        .await
        .expect("get success conversation")
        .expect("success conversation");
    assert_eq!(succeeded.conversation.version, 3);
    assert_eq!(
        succeeded.turns[0].status,
        LocalConversationTurnStatus::Succeeded
    );
    assert_eq!(succeeded.messages.len(), 2);
    assert_eq!(
        succeeded.messages[1].role,
        LocalConversationMessageRole::Assistant
    );
    assert_eq!(succeeded.messages[1].content, json!({"answer": "done"}));

    create(&storage, "conversation-cancel").await;
    let cancel_turn = turn("conversation-cancel", 1, "cancel");
    let started = start(&storage, &cancel_turn, 5_000)
        .await
        .expect("start cancelled Turn");
    storage
        .cancel_run(
            &idempotency("cancel-run"),
            &started.run.run_id,
            Some(started.run.version),
            "user cancelled",
            "event-cancel",
            6_000,
        )
        .await
        .expect("cancel Run");
    let cancelled = storage
        .get_conversation("conversation-cancel")
        .await
        .expect("get cancelled conversation")
        .expect("cancelled conversation");
    assert_eq!(cancelled.conversation.version, 3);
    assert_eq!(
        cancelled.turns[0].status,
        LocalConversationTurnStatus::Cancelled
    );
    assert_eq!(cancelled.messages.len(), 1);

    create(&storage, "conversation-failure").await;
    let failure_turn = turn("conversation-failure", 1, "failure");
    start(&storage, &failure_turn, 7_000)
        .await
        .expect("start failed Turn");
    let claim = storage
        .claim_next_run(
            &idempotency("claim-failure"),
            "worker-1",
            "token-failure",
            8_000,
            18_000,
            "event-claim-failure",
        )
        .await
        .expect("claim")
        .expect("claimed Run");
    storage
        .apply_transition(
            &idempotency("complete-failure"),
            &chatos_local_agent_ports::RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::Failed,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(json!({"error": "provider rejected request"})),
                event_id: "event-failure".to_string(),
                event_type: "run_failed".to_string(),
                event_payload: json!({"error": "provider rejected request"}),
                occurred_at_unix_ms: 9_000,
            },
        )
        .await
        .expect("fail Run");
    let failed = storage
        .get_conversation("conversation-failure")
        .await
        .expect("get failed conversation")
        .expect("failed conversation");
    assert_eq!(failed.conversation.version, 3);
    assert_eq!(failed.turns[0].status, LocalConversationTurnStatus::Failed);
    assert_eq!(failed.messages.len(), 1);
}
