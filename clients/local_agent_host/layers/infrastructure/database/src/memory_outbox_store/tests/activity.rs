#[tokio::test]
async fn live_conversation_records_are_committed_unsynced_and_scope_isolated() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    storage
        .configure_memory_projection("user-1", "chatos", 1)
        .await
        .unwrap();
    for (id, tenant, source, thread, conversation, role, response_status) in [
        (
            "pending",
            "user-1",
            "chatos",
            "agent-1",
            "conversation-1",
            "user",
            "completed",
        ),
        (
            "syncing",
            "user-1",
            "chatos",
            "agent-1",
            "conversation-1",
            "assistant",
            "completed",
        ),
        (
            "retry",
            "user-1",
            "chatos",
            "agent-1",
            "conversation-1",
            "assistant",
            "completed",
        ),
        (
            "synced",
            "user-1",
            "chatos",
            "agent-1",
            "conversation-1",
            "assistant",
            "completed",
        ),
        (
            "other-owner",
            "user-2",
            "chatos",
            "agent-1",
            "conversation-1",
            "assistant",
            "completed",
        ),
        (
            "other-source",
            "user-1",
            "other",
            "agent-1",
            "conversation-1",
            "assistant",
            "completed",
        ),
        (
            "other-thread",
            "user-1",
            "chatos",
            "agent-2",
            "conversation-1",
            "assistant",
            "completed",
        ),
        (
            "other-conversation",
            "user-1",
            "chatos",
            "agent-1",
            "conversation-2",
            "assistant",
            "completed",
        ),
        (
            "tool",
            "user-1",
            "chatos",
            "agent-1",
            "conversation-1",
            "tool",
            "completed",
        ),
        (
            "tool-call",
            "user-1",
            "chatos",
            "agent-1",
            "conversation-1",
            "assistant",
            "tool_calls",
        ),
    ] {
        storage
            .enqueue_memory_record(
                id,
                tenant,
                source,
                thread,
                &json!({
                    "role":role, "response_status":response_status,
                    "metadata":{"conversation_id":conversation}
                }),
                1_000,
            )
            .await
            .unwrap();
    }
    sqlx::query("UPDATE local_memory_outbox SET status = CASE record_id WHEN 'synced' THEN 'synced' WHEN 'syncing' THEN 'syncing' WHEN 'retry' THEN 'retry_scheduled' ELSE status END")
            .execute(&storage.pool).await.unwrap();
    // A valid, live model claim may contain a reply that has not committed.
    sqlx::query(
            "INSERT INTO local_agent_runs(\
             run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
             model_config_ref, model_config_revision, capability_policy_revision, input_json, \
             status, iteration, max_iterations, version, claim_token, claim_until_unix_ms, \
             created_at_unix_ms, updated_at_unix_ms) \
             VALUES('run-1', 'user-1', 'conversation', 'conversation-1', 'main_chat', \
             'model-1', 'revision-1', 'capability-1', '{}', 'model_running', 1, 12, 1, 'claim', 9000, 1, 1)",
        ).execute(&storage.pool).await.unwrap();
    storage
        .stage_step_memory_record(
            &LocalMemoryRunClaim {
                run_id: "run-1".into(),
                claim_token: "claim".into(),
                version: 1,
            },
            "staged",
            "user-1",
            "chatos",
            "agent-1",
            &json!({"role":"assistant", "metadata":{"conversation_id":"conversation-1"}}),
            1_000,
        )
        .await
        .unwrap();
    let records = storage
        .list_pending_conversation_memory_records("user-1", "agent-1", "conversation-1")
        .await
        .unwrap();
    assert_eq!(
        records
            .iter()
            .map(|record| record.record_id.as_str())
            .collect::<Vec<_>>(),
        vec!["pending", "retry", "syncing"]
    );
    assert!(
        storage
            .list_pending_conversation_memory_records("user-2", "agent-1", "conversation-1")
            .await
            .unwrap()
            .is_empty(),
        "unconfigured owner must not receive another owner's source"
    );
    assert!(storage
        .list_pending_conversation_memory_records("", "agent-1", "conversation-1")
        .await
        .is_err());
}

#[tokio::test]
async fn committed_outbox_work_advances_a_retained_activity_revision() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let observed = storage.memory_outbox_revision();
    for index in 1..=2 {
        storage
            .enqueue_memory_record(
                &format!("message-{index}"),
                "user-1",
                "local_agent",
                "conversation-1",
                &json!({"message_id": format!("message-{index}")}),
                1_000 + index,
            )
            .await
            .expect("enqueue");
    }

    let advanced = storage
        .wait_for_memory_outbox_revision(observed)
        .await
        .expect("retained revision");
    assert_eq!(advanced, observed + 2);

    let claimed = storage
        .claim_next_memory_record("user-1", "claim-1", 2_000, 12_000)
        .await
        .expect("claim")
        .expect("record");
    storage
        .complete_memory_record(
            &claimed.source_id,
            &claimed.record_id,
            "claim-1",
            claimed.version,
            3_000,
        )
        .await
        .expect("complete");
    assert_eq!(storage.memory_outbox_revision(), advanced);
}

#[tokio::test]
async fn rolled_back_outbox_work_does_not_publish_activity() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let observed = storage.memory_outbox_revision();
    let mut connection = storage.acquire_write().await.expect("write connection");
    SqliteClientStorage::begin_immediate(&mut connection)
        .await
        .expect("begin");
    enqueue_on(
        &mut connection,
        "rolled-back-message",
        "user-1",
        "local_agent",
        "conversation-1",
        &json!({"message_id": "rolled-back-message"}),
        1_000,
    )
    .await
    .expect("stage outbox row");
    let rollback = SqliteClientStorage::finish_write::<()>(
        &mut connection,
        Err(ClientStorageError::Conflict("force rollback".to_string())),
    )
    .await;
    assert!(rollback.is_err());
    assert_eq!(storage.memory_outbox_revision(), observed);
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(50),
        storage.wait_for_memory_outbox_revision(observed),
    )
    .await
    .is_err());
    drop(connection);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_memory_outbox WHERE record_id = 'rolled-back-message'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("count rolled back row");
    assert_eq!(count, 0);
}
