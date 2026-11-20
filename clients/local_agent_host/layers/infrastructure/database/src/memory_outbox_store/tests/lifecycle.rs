#[tokio::test]
async fn outbox_is_immutable_claimed_and_completed_with_cas() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let payload = json!({"message_id": "message-1", "content": "hello"});
    let queued = storage
        .enqueue_memory_record(
            "message-1",
            "user-1",
            "local_agent",
            "conversation-1",
            &payload,
            1_000,
        )
        .await
        .expect("enqueue");
    assert_eq!(queued.status, LocalMemoryOutboxStatus::Pending);
    let replay = storage
        .enqueue_memory_record(
            "message-1",
            "user-1",
            "local_agent",
            "conversation-1",
            &payload,
            1_001,
        )
        .await
        .expect("replay");
    assert_eq!(queued, replay);
    assert!(storage
        .enqueue_memory_record(
            "message-1",
            "user-1",
            "local_agent",
            "conversation-1",
            &json!({"content": "changed"}),
            1_002,
        )
        .await
        .is_err());

    let claimed = storage
        .claim_next_memory_record("user-1", "claim-1", 2_000, 12_000)
        .await
        .expect("claim")
        .expect("record");
    assert_eq!(claimed.status, LocalMemoryOutboxStatus::Syncing);
    assert_eq!(claimed.version, 2);
    let synced = storage
        .complete_memory_record(
            "local_agent",
            "message-1",
            "claim-1",
            claimed.version,
            3_000,
        )
        .await
        .expect("complete");
    assert_eq!(synced.status, LocalMemoryOutboxStatus::Synced);
    assert_eq!(synced.attempt_count, 1);
    assert!(storage
        .complete_memory_record(
            "local_agent",
            "message-1",
            "claim-1",
            claimed.version,
            3_001,
        )
        .await
        .is_err());
}

#[tokio::test]
async fn synced_tool_result_replay_keeps_original_created_at() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let original = json!({
        "message_id": "run-1:tool:batch-1:0",
        "role": "tool",
        "content": "same tool result",
        "created_at": "2026-11-20T08:35:07Z"
    });
    storage
        .enqueue_memory_record(
            "run-1:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-1",
            &original,
            1_000,
        )
        .await
        .expect("enqueue");
    let claimed = storage
        .claim_next_memory_record("user-1", "claim-1", 2_000, 12_000)
        .await
        .expect("claim")
        .expect("record");
    storage
        .complete_memory_record(
            "chatos",
            "run-1:tool:batch-1:0",
            "claim-1",
            claimed.version,
            3_000,
        )
        .await
        .expect("complete");

    let replay = json!({
        "message_id": "run-1:tool:batch-1:0",
        "role": "tool",
        "content": "same tool result",
        "created_at": "2026-11-20T08:35:39Z"
    });
    let stored = storage
        .enqueue_memory_record(
            "run-1:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-1",
            &replay,
            4_000,
        )
        .await
        .expect("idempotent replay");
    assert_eq!(stored.status, LocalMemoryOutboxStatus::Synced);
    assert_eq!(stored.payload, original);

    let changed = json!({
        "message_id": "run-1:tool:batch-1:0",
        "role": "tool",
        "content": "changed tool result",
        "created_at": "2026-11-20T08:35:40Z"
    });
    assert!(storage
        .enqueue_memory_record(
            "run-1:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-1",
            &changed,
            5_000,
        )
        .await
        .is_err());
}

#[tokio::test]
async fn completing_conversation_record_publishes_exact_memory_transcript_event() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    sqlx::query(
        "INSERT INTO local_agent_runs(\
             run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
             model_config_ref, model_config_revision, capability_policy_revision, input_json, \
             status, iteration, max_iterations, version, created_at_unix_ms, updated_at_unix_ms) \
             VALUES('run-1', 'user-1', 'conversation', 'conversation-1', 'main_chat', \
             'model-1', 'revision-1', 'capability-1', '{}', 'succeeded', 1, 12, 1, 1, 1)",
    )
    .execute(&storage.pool)
    .await
    .expect("insert run");
    let payload = json!({
        "message_id": "assistant-1",
        "role": "assistant",
        "content": "done",
        "metadata": {
            "conversation_id": "conversation-1",
            "run_id": "run-1"
        }
    });
    storage
        .enqueue_memory_record(
            "assistant-1",
            "user-1",
            "local_agent",
            "client-agent:agent:jiguli",
            &payload,
            1_000,
        )
        .await
        .expect("enqueue");
    let claimed = storage
        .claim_next_memory_record("user-1", "claim-1", 2_000, 12_000)
        .await
        .expect("claim")
        .expect("record");
    storage
        .complete_memory_record(
            "local_agent",
            "assistant-1",
            "claim-1",
            claimed.version,
            3_000,
        )
        .await
        .expect("complete");

    let events = storage
        .list_events_for_owner(
            "user-1",
            0,
            10,
            None,
            None,
            false,
            chatos_local_agent_protocol::LocalAgentEventPayloadMode::Routing,
        )
        .await
        .expect("events");
    let event = events
        .iter()
        .find(|event| event.event_type == "memory_transcript_record_synced")
        .expect("Memory transcript event");
    assert_eq!(event.run_id, "run-1");
    assert_eq!(event.payload["conversation_id"], "conversation-1");
}

#[tokio::test]
async fn synced_legacy_record_accepts_additive_memory_routing_metadata_on_upgrade_replay() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let legacy = json!({
        "message_id": "run-1:tool:batch-1:0",
        "content": "same tool result",
        "created_at": "2026-11-20T08:35:07Z",
        "metadata": {
            "tenant_id": "user-1",
            "run_id": "run-1",
            "toolName": "read_file"
        }
    });
    storage
        .enqueue_memory_record(
            "run-1:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-1",
            &legacy,
            1_000,
        )
        .await
        .expect("legacy enqueue");
    let claimed = storage
        .claim_next_memory_record("user-1", "claim-1", 2_000, 12_000)
        .await
        .expect("claim")
        .expect("record");
    storage
        .complete_memory_record(
            "chatos",
            "run-1:tool:batch-1:0",
            "claim-1",
            claimed.version,
            3_000,
        )
        .await
        .expect("complete");

    let enriched = json!({
        "message_id": "run-1:tool:batch-1:0",
        "content": "same tool result",
        "created_at": "2026-11-20T08:35:39Z",
        "metadata": {
            "tenant_id": "user-1",
            "run_id": "run-1",
            "toolName": "read_file",
            "agent_id": "jiguli",
            "project_id": null,
            "memory_project_id": null,
            "thread_project_aggregation_id": null,
            "thread_kind": "agent_task"
        }
    });
    let replay = storage
        .enqueue_memory_record(
            "run-1:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-1",
            &enriched,
            4_000,
        )
        .await
        .expect("upgrade replay");

    assert_eq!(replay.status, LocalMemoryOutboxStatus::Synced);
    assert_eq!(replay.payload, legacy);

    let concrete_identity = json!({
        "message_id": "run-2:tool:batch-1:0",
        "content": "same tool result",
        "metadata": {
            "tenant_id": "user-1",
            "run_id": "run-2",
            "toolName": "read_file",
            "agent_id": "agent-1"
        }
    });
    storage
        .enqueue_memory_record(
            "run-2:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-2",
            &concrete_identity,
            5_000,
        )
        .await
        .expect("concrete identity enqueue");
    let conflicting_identity = json!({
        "message_id": "run-2:tool:batch-1:0",
        "content": "same tool result",
        "metadata": {
            "tenant_id": "user-1",
            "run_id": "run-2",
            "toolName": "read_file",
            "agent_id": "different-agent"
        }
    });
    assert!(storage
        .enqueue_memory_record(
            "run-2:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-2",
            &conflicting_identity,
            6_000,
        )
        .await
        .is_err());

    let missing_identity = json!({
        "message_id": "run-2:tool:batch-1:0",
        "content": "same tool result",
        "metadata": {
            "tenant_id": "user-1",
            "run_id": "run-2",
            "toolName": "read_file"
        }
    });
    assert!(storage
        .enqueue_memory_record(
            "run-2:tool:batch-1:0",
            "user-1",
            "chatos",
            "task-2",
            &missing_identity,
            7_000,
        )
        .await
        .is_err());
}

#[tokio::test]
async fn pending_legacy_record_is_enriched_before_first_sync() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let legacy = json!({
        "message_id": "message-1",
        "content": "same",
        "created_at": "2026-11-20T08:35:07Z",
        "metadata": {"tenant_id": "user-1"}
    });
    let enriched = json!({
        "message_id": "message-1",
        "content": "same",
        "created_at": "2026-11-20T08:35:39Z",
        "metadata": {
            "tenant_id": "user-1",
            "agent_id": "agent-1",
            "memory_project_id": "project-1"
        }
    });
    let queued = storage
        .enqueue_memory_record("message-1", "user-1", "chatos", "task-1", &legacy, 1_000)
        .await
        .expect("legacy enqueue");
    let upgraded = storage
        .enqueue_memory_record("message-1", "user-1", "chatos", "task-1", &enriched, 2_000)
        .await
        .expect("upgrade replay");

    assert_eq!(upgraded.status, LocalMemoryOutboxStatus::Pending);
    let mut expected = enriched;
    expected["created_at"] = legacy["created_at"].clone();
    assert_eq!(upgraded.payload, expected);
    assert!(upgraded.version > queued.version);
}
