#[tokio::test]
async fn failed_and_expired_claims_are_retryable_after_their_deadline() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .enqueue_memory_record(
            "message-1",
            "user-1",
            "local_agent",
            "conversation-1",
            &json!({"message_id": "message-1"}),
            1_000,
        )
        .await
        .expect("enqueue");
    let claimed = storage
        .claim_next_memory_record("user-1", "claim-1", 2_000, 3_000)
        .await
        .expect("claim")
        .expect("record");
    assert_eq!(
        storage
            .next_memory_retry_at("user-1")
            .await
            .expect("active claim deadline"),
        Some(3_000)
    );
    assert!(storage
        .claim_next_memory_record("user-1", "claim-2", 2_500, 3_500)
        .await
        .expect("no second claim")
        .is_none());
    let reclaimed = storage
        .claim_next_memory_record("user-1", "claim-3", 3_000, 4_000)
        .await
        .expect("reclaim")
        .expect("record");
    assert!(reclaimed.version > claimed.version);
    let retry = storage
        .retry_memory_record(
            "local_agent",
            "message-1",
            "claim-3",
            reclaimed.version,
            "offline",
            8_000,
            3_500,
        )
        .await
        .expect("retry");
    assert_eq!(retry.status, LocalMemoryOutboxStatus::RetryScheduled);
    assert_eq!(
        storage
            .next_memory_retry_at("user-1")
            .await
            .expect("deadline"),
        Some(8_000)
    );
    assert!(storage
        .claim_next_memory_record("user-1", "claim-4", 7_999, 9_000)
        .await
        .expect("not due")
        .is_none());
    assert!(storage
        .claim_next_memory_record("user-1", "claim-5", 8_000, 9_000)
        .await
        .expect("due")
        .is_some());
}

#[tokio::test]
async fn claim_and_expired_lease_recovery_are_tenant_scoped() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    for (record_id, tenant_id) in [("other", "user-2"), ("owned", "user-1")] {
        storage
            .enqueue_memory_record(
                record_id,
                tenant_id,
                "local_agent",
                "conversation-1",
                &json!({"message_id": record_id}),
                1_000,
            )
            .await
            .expect("enqueue");
    }
    let other = storage
        .claim_next_memory_record("user-2", "claim-other", 2_000, 3_000)
        .await
        .expect("claim other")
        .expect("other record");
    assert_eq!(other.record_id, "other");

    let owned = storage
        .claim_next_memory_record("user-1", "claim-owned", 4_000, 14_000)
        .await
        .expect("claim owned")
        .expect("owned record");
    assert_eq!(owned.record_id, "owned");
    let other_status = storage
        .get_memory_sync_status("user-2", "local_agent")
        .await
        .expect("other status");
    assert_eq!(other_status.syncing_count, 1);
    assert_eq!(other_status.retry_scheduled_count, 0);
}

#[tokio::test]
async fn sync_status_is_scoped_and_never_exposes_payloads() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    for (record_id, tenant_id, now) in [
        ("message-1", "user-1", 1_000),
        ("message-2", "user-1", 2_000),
        ("message-other", "user-2", 3_000),
    ] {
        storage
            .enqueue_memory_record(
                record_id,
                tenant_id,
                "local_agent",
                "conversation-1",
                &json!({"message_id": record_id, "secret_content": "not returned"}),
                now,
            )
            .await
            .expect("enqueue");
    }
    let synced = storage
        .claim_next_memory_record("user-1", "claim-1", 4_000, 14_000)
        .await
        .expect("claim")
        .expect("record");
    storage
        .complete_memory_record(
            &synced.source_id,
            &synced.record_id,
            "claim-1",
            synced.version,
            5_000,
        )
        .await
        .expect("complete");
    let failed = storage
        .claim_next_memory_record("user-1", "claim-2", 6_000, 16_000)
        .await
        .expect("claim")
        .expect("record");
    storage
        .retry_memory_record(
            &failed.source_id,
            &failed.record_id,
            "claim-2",
            failed.version,
            "Memory offline",
            20_000,
            7_000,
        )
        .await
        .expect("retry");

    let status = storage
        .get_memory_sync_status("user-1", "local_agent")
        .await
        .expect("status");
    assert_eq!(status.synced_count, 1);
    assert_eq!(status.retry_scheduled_count, 1);
    assert_eq!(status.pending_count, 0);
    assert_eq!(status.unsynced_count(), 1);
    assert_eq!(status.oldest_unsynced_at_unix_ms, Some(2_000));
    assert_eq!(status.next_retry_at_unix_ms, Some(20_000));
    assert_eq!(status.last_synced_at_unix_ms, Some(5_000));
    assert_eq!(status.last_error.as_deref(), Some("Memory offline"));
    assert_eq!(status.last_error_at_unix_ms, Some(7_000));

    let other = storage
        .get_memory_sync_status("user-2", "local_agent")
        .await
        .expect("other status");
    assert_eq!(other.pending_count, 1);
    assert_eq!(other.synced_count, 0);
    assert_eq!(other.last_error, None);
}

#[tokio::test]
async fn mutable_program_projection_rearms_a_synced_record_with_new_content() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let first = json!({
        "conversation_id": "client-agent:agent:jiguli",
        "message_id": "callback-1",
        "role": "assistant",
        "content": "running",
        "metadata": {"memory_projection_mutable": true},
    });
    storage
        .enqueue_memory_record(
            "callback-1",
            "user-1",
            "chatos",
            "client-agent:agent:jiguli",
            &first,
            1_000,
        )
        .await
        .expect("enqueue running projection");
    let claimed = storage
        .claim_next_memory_record("user-1", "claim-1", 2_000, 3_000)
        .await
        .unwrap()
        .unwrap();
    storage
        .complete_memory_record("chatos", "callback-1", "claim-1", claimed.version, 2_500)
        .await
        .expect("complete first projection");

    let terminal = json!({
        "conversation_id": "client-agent:agent:jiguli",
        "message_id": "callback-1",
        "role": "assistant",
        "content": "completed",
        "metadata": {"memory_projection_mutable": true},
    });
    let updated = storage
        .enqueue_memory_record(
            "callback-1",
            "user-1",
            "chatos",
            "client-agent:agent:jiguli",
            &terminal,
            4_000,
        )
        .await
        .expect("rearm terminal projection");
    assert_eq!(updated.status, LocalMemoryOutboxStatus::Pending);
    assert_eq!(updated.payload["content"], "completed");
    assert_eq!(updated.attempt_count, 0);
    assert!(updated.version > claimed.version);
}
