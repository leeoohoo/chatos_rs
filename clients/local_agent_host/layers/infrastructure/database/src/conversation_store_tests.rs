// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_local_agent_ports::{LocalAgentRunStore, RunTransition};
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, GuideConversationTurnCommand, LocalAgentRunStatus,
    LocalConversationAttachmentSpec, ResumeConversationTurnCommand,
};
use serde_json::{json, Value};

fn idempotency(command_id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: command_id.to_string(),
        request_fingerprint: command_id.to_string(),
        persist_receipt: true,
    }
}

fn conversation(conversation_id: &str) -> CreateConversationCommand {
    CreateConversationCommand {
        conversation_id: conversation_id.to_string(),
        owner_user_id: "user-1".to_string(),
        title: "Local conversation".to_string(),
        resource: None,
    }
}

fn turn(
    conversation_id: &str,
    expected_conversation_version: u64,
    suffix: &str,
) -> StartConversationTurnCommand {
    StartConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
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

async fn wait_for_user(
    storage: &SqliteClientStorage,
    run_id: &str,
    suffix: &str,
    now: i64,
) -> LocalAgentRunRecord {
    let claim = storage
        .claim_next_run(
            &idempotency(&format!("claim-{suffix}")),
            "user-1",
            "worker-1",
            &format!("token-{suffix}"),
            now,
            now + 10_000,
            &format!("event-claim-{suffix}"),
        )
        .await
        .expect("claim")
        .expect("claimed Run");
    assert_eq!(claim.run.run_id, run_id);
    storage
        .apply_transition(
            &idempotency(&format!("wait-{suffix}")),
            &RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::WaitingUser,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: Some(json!({"prompt": "continue?"})),
                clear_continuation_input: true,
                terminal_outcome: None,
                event_id: format!("event-wait-{suffix}"),
                event_type: "run_waiting_user".to_string(),
                event_payload: json!({"prompt": "continue?"}),
                occurred_at_unix_ms: now + 1_000,
            },
        )
        .await
        .expect("wait for user")
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
        .get_conversation("user-1", "conversation-1")
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
            "user-1",
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
        .get_conversation("user-1", "conversation-success")
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
        .get_conversation("user-1", "conversation-cancel")
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
            "user-1",
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
        .get_conversation("user-1", "conversation-failure")
        .await
        .expect("get failed conversation")
        .expect("failed conversation");
    assert_eq!(failed.conversation.version, 3);
    assert_eq!(failed.turns[0].status, LocalConversationTurnStatus::Failed);
    assert_eq!(failed.messages.len(), 1);
}

#[tokio::test]
async fn resume_turn_is_atomic_idempotent_and_rejects_stale_versions() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create(&storage, "conversation-resume").await;
    let initial = turn("conversation-resume", 1, "resume");
    start(&storage, &initial, 2_000).await.expect("start Turn");
    let waiting = wait_for_user(&storage, &initial.run_id, "resume", 3_000).await;

    let mut command = ResumeConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-resume".to_string(),
        expected_conversation_version: 2,
        turn_id: initial.turn_id.clone(),
        expected_run_version: waiting.version,
        expected_run_status: LocalAgentRunStatus::WaitingUser,
        message_id: "message-resume-answer".to_string(),
        message: "continue locally".to_string(),
        message_metadata: json!({"source": "ask_user"}),
        attachments: vec![LocalConversationAttachmentSpec {
            attachment_id: "attachment-resume-answer".to_string(),
            display_name: "answer.txt".to_string(),
            media_type: "text/plain".to_string(),
            byte_size: 5,
            sha256: "b".repeat(64),
            authorized_local_ref: "local-attachment:answer".to_string(),
            metadata: json!({}),
        }],
        reason: "user replied".to_string(),
    };
    let continuation = json!({
        "type": "resume",
        "reason": "user replied",
        "input": {"message": "continue locally", "attachments": command.attachments}
    });

    command.expected_conversation_version = 3;
    assert!(matches!(
        storage
            .resume_conversation_turn(
                &idempotency("resume-stale-conversation"),
                &command,
                &continuation,
                "event-resume-stale",
                5_000,
            )
            .await,
        Err(ClientStorageError::Conflict(_))
    ));
    command.expected_conversation_version = 2;
    command.expected_run_version += 1;
    assert!(matches!(
        storage
            .resume_conversation_turn(
                &idempotency("resume-stale-run"),
                &command,
                &continuation,
                "event-resume-stale-run",
                5_000,
            )
            .await,
        Err(ClientStorageError::Conflict(_))
    ));
    let unchanged = storage
        .get_conversation("user-1", "conversation-resume")
        .await
        .expect("get unchanged conversation")
        .expect("conversation");
    assert_eq!(unchanged.conversation.version, 2);
    assert_eq!(unchanged.messages.len(), 1);
    assert_eq!(unchanged.attachments.len(), 0);

    command.expected_run_version = waiting.version;
    let receipt = idempotency("resume-conversation-turn");
    let resumed = storage
        .resume_conversation_turn(&receipt, &command, &continuation, "event-resume", 6_000)
        .await
        .expect("resume Turn");
    assert_eq!(resumed.conversation.version, 3);
    assert_eq!(resumed.run.status, LocalAgentRunStatus::ContinuationReady);
    assert_eq!(resumed.run.continuation_input, Some(continuation.clone()));
    assert_eq!(resumed.message.as_ref().expect("message").ordinal, 2);
    assert_eq!(resumed.attachments.len(), 1);

    let replay = storage
        .resume_conversation_turn(&receipt, &command, &continuation, "ignored-event", 7_000)
        .await
        .expect("replay resume");
    assert_eq!(replay, resumed);
    let after_replay = storage
        .get_conversation("user-1", "conversation-resume")
        .await
        .expect("get replayed conversation")
        .expect("conversation");
    assert_eq!(after_replay.messages.len(), 2);
    assert_eq!(after_replay.attachments.len(), 1);
}

#[tokio::test]
async fn cancel_turn_reconciles_the_owned_run_without_assistant_message() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create(&storage, "conversation-stop").await;
    let initial = turn("conversation-stop", 1, "stop");
    let started = start(&storage, &initial, 2_000).await.expect("start Turn");
    let command = CancelConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-stop".to_string(),
        expected_conversation_version: 2,
        turn_id: initial.turn_id,
        expected_run_version: Some(started.run.version),
        reason: "user stopped".to_string(),
    };
    let receipt = idempotency("cancel-conversation-turn");
    let cancelled = storage
        .cancel_conversation_turn(&receipt, &command, "event-stop", 3_000)
        .await
        .expect("cancel Turn");
    assert_eq!(cancelled.conversation.version, 3);
    assert_eq!(cancelled.run.status, LocalAgentRunStatus::Cancelled);
    assert_eq!(
        cancelled.turn.status,
        LocalConversationTurnStatus::Cancelled
    );
    assert!(cancelled.message.is_none());
    assert!(cancelled.attachments.is_empty());

    let replay = storage
        .cancel_conversation_turn(&receipt, &command, "ignored-event", 4_000)
        .await
        .expect("replay cancel");
    assert_eq!(replay, cancelled);
    let detail = storage
        .get_conversation("user-1", "conversation-stop")
        .await
        .expect("get cancelled conversation")
        .expect("conversation");
    assert_eq!(detail.messages.len(), 1);
}

#[tokio::test]
async fn history_pages_are_bounded_chronological_and_cursor_stable() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create(&storage, "conversation-history").await;
    let mut expected_version = 1;
    for (index, suffix) in ["one", "two", "three"].into_iter().enumerate() {
        let mut next = turn("conversation-history", expected_version, suffix);
        if suffix == "two" {
            next.attachments.push(LocalConversationAttachmentSpec {
                attachment_id: "attachment-history-two".to_string(),
                display_name: "two.txt".to_string(),
                media_type: "text/plain".to_string(),
                byte_size: 3,
                sha256: "c".repeat(64),
                authorized_local_ref: "local-attachment:history-two".to_string(),
                metadata: json!({}),
            });
        }
        let started = start(&storage, &next, 10_000 + index as i64)
            .await
            .expect("start history Turn");
        expected_version += 1;
        storage
            .cancel_run(
                &idempotency(&format!("cancel-history-{suffix}")),
                &started.run.run_id,
                Some(started.run.version),
                "finish fixture",
                &format!("event-cancel-history-{suffix}"),
                20_000 + index as i64,
            )
            .await
            .expect("cancel history Run");
        expected_version += 1;
    }

    let latest = storage
        .get_conversation_history("user-1", "conversation-history", None, 2)
        .await
        .expect("latest history page");
    assert_eq!(
        latest
            .messages
            .iter()
            .map(|message| message.ordinal)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(latest.next_before_ordinal, Some(2));
    assert_eq!(latest.turns.len(), 2);
    assert_eq!(latest.attachments.len(), 1);
    assert_eq!(latest.attachments[0].message_id, "message-two");

    let older = storage
        .get_conversation_history(
            "user-1",
            "conversation-history",
            latest.next_before_ordinal,
            2,
        )
        .await
        .expect("older history page");
    assert_eq!(older.messages.len(), 1);
    assert_eq!(older.messages[0].ordinal, 1);
    assert_eq!(older.next_before_ordinal, None);
    assert!(older.attachments.is_empty());
}

#[tokio::test]
async fn guidance_interrupts_an_active_claim_and_is_delivered_once() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    create(&storage, "conversation-guidance").await;
    let initial = turn("conversation-guidance", 1, "guidance");
    start(&storage, &initial, 2_000).await.expect("start Turn");
    let claim = storage
        .claim_next_run(
            &idempotency("claim-before-guidance"),
            "user-1",
            "worker-1",
            "token-before-guidance",
            3_000,
            13_000,
            "event-claim-before-guidance",
        )
        .await
        .expect("claim")
        .expect("claimed Run");
    let command = GuideConversationTurnCommand {
        owner_user_id: "user-1".to_string(),
        conversation_id: "conversation-guidance".to_string(),
        expected_conversation_version: 2,
        turn_id: initial.turn_id.clone(),
        expected_run_version: Some(claim.run.version),
        message_id: "message-guidance-follow-up".to_string(),
        message: "also inspect the tests".to_string(),
        message_metadata: json!({"source": "guidance"}),
        attachments: Vec::new(),
    };
    let receipt = idempotency("guide-conversation-turn");
    let guided = storage
        .guide_conversation_turn(&receipt, &command, "event-guidance", 4_000)
        .await
        .expect("queue guidance");
    assert_eq!(guided.conversation.version, 3);
    assert_eq!(guided.run.status, LocalAgentRunStatus::ContinuationReady);
    assert!(guided.run.claim_token.is_none());
    assert_eq!(guided.message.as_ref().expect("message").ordinal, 2);

    let replay = storage
        .guide_conversation_turn(&receipt, &command, "ignored-event", 5_000)
        .await
        .expect("replay guidance");
    assert_eq!(replay, guided);
    let late = storage
        .apply_transition(
            &idempotency("late-guidance-commit"),
            &RunTransition {
                run_id: claim.run.run_id.clone(),
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
                terminal_outcome: Some(json!({"answer": "stale"})),
                event_id: "event-late-guidance".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: json!({"answer": "stale"}),
                occurred_at_unix_ms: 5_000,
            },
        )
        .await;
    assert!(matches!(late, Err(ClientStorageError::Conflict(_))));

    let guided_claim = storage
        .claim_next_run(
            &idempotency("claim-after-guidance"),
            "user-1",
            "worker-1",
            "token-after-guidance",
            6_000,
            16_000,
            "event-claim-after-guidance",
        )
        .await
        .expect("claim guidance")
        .expect("guidance Run");
    assert_eq!(
        guided_claim.run.continuation_input.as_ref().expect("input")["type"],
        "guidance"
    );
    assert_eq!(
        guided_claim.run.continuation_input.as_ref().expect("input")["guidance"][0]["message"],
        "also inspect the tests"
    );
}
