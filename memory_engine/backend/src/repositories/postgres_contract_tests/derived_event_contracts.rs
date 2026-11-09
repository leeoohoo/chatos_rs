// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::UpsertRecordInput;

fn terminal_record(id: &str) -> UpsertRecordInput {
    UpsertRecordInput {
        id: id.to_string(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: format!("content-{id}"),
        structured_payload: None,
        metadata: Some(json!({
            "conversation_turn_id": format!("turn-{id}"),
            "memory_project_id": "alpha",
            "memory_terminal_summary_requested": true
        })),
        summary_status: None,
        summary_id: None,
        summarized_at: None,
        created_at: "2026-11-09T00:00:00Z".to_string(),
    }
}

async fn setup(
    pool: &crate::db::Db,
) -> (
    String,
    String,
    String,
    crate::models::BatchSyncRecordsRequest,
) {
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{thread_id}");
    crate::repositories::threads::upsert_thread(
        pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), subject]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create derived event thread");
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        records: vec![terminal_record(format!("record-{suffix}").as_str())],
    };
    crate::repositories::records::batch_sync_records_with_partition_dispatch(
        &test_config("unused".to_string()),
        pool,
        &thread_id,
        &request,
        true,
    )
    .await
    .expect("create derived event");
    (tenant, source, thread_id, request)
}

async fn claim(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    owner: &str,
    lease_seconds: i64,
) -> crate::repositories::derived_events::DerivedEventOutbox {
    let events = crate::repositories::derived_events::claim_publish_batch_for_scope(
        pool,
        owner,
        lease_seconds,
        10,
        Some(tenant),
        Some(source),
    )
    .await
    .expect("claim scoped derived event");
    assert_eq!(events.len(), 1);
    events.into_iter().next().unwrap()
}

#[tokio::test]
async fn derived_event_lease_takeover_and_publish_are_cas_guarded() {
    let Some(pool) = pool().await else {
        return;
    };
    let (tenant, source, _thread_id, _request) = setup(&pool).await;
    let first = claim(&pool, &tenant, &source, "publisher-1", 300).await;
    let blocked = crate::repositories::derived_events::claim_publish_batch_for_scope(
        &pool,
        "publisher-2",
        300,
        10,
        Some(&tenant),
        Some(&source),
    )
    .await
    .expect("active lease blocks takeover");
    assert!(blocked.is_empty());

    sqlx::query(
        "UPDATE engine_derived_event_outbox SET lease_expires_at=now()-interval '1 second' \
         WHERE tenant_id=$1 AND source_id=$2 AND event_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&first.event_id)
    .execute(&pool)
    .await
    .expect("expire publisher lease");
    let takeover = claim(&pool, &tenant, &source, "publisher-2", 300).await;
    assert_eq!(takeover.event_id, first.event_id);
    assert_eq!(takeover.attempt_count, 2);
    assert_ne!(takeover.lease_token, first.lease_token);
    assert!(
        !crate::repositories::derived_events::mark_published(&pool, &first)
            .await
            .expect("stale publisher CAS")
    );
    assert!(
        crate::repositories::derived_events::mark_published(&pool, &takeover)
            .await
            .expect("current publisher CAS")
    );
    assert!(
        !crate::repositories::derived_events::mark_published(&pool, &takeover)
            .await
            .expect("duplicate publisher CAS")
    );
    let state: (String, i64, bool) = sqlx::query_as(
        "SELECT status,summary_dispatch_published_version,summary_dispatch_pending \
         FROM engine_derived_event_outbox event JOIN engine_thread_summary_partitions partition \
         ON partition.tenant_id=event.tenant_id AND partition.source_id=event.source_id \
         AND partition.project_partition_key=event.project_partition_key \
         WHERE event.tenant_id=$1 AND event.source_id=$2 AND event.event_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&first.event_id)
    .fetch_one(&pool)
    .await
    .expect("load published state");
    assert_eq!(state, ("published".to_string(), 1, false));
}

#[tokio::test]
async fn derived_event_consumer_is_authoritative_idempotent_and_monotonic() {
    let Some(pool) = pool().await else {
        return;
    };
    let (tenant, source, thread_id, mut request) = setup(&pool).await;
    let unauthorized = crate::repositories::derived_events::consume_thread_summary_request(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        1,
        "v2",
    )
    .await
    .expect_err("non-authoritative consumer must fail closed");
    assert!(unauthorized.contains("not authoritative"));
    assert!(
        crate::repositories::derived_events::consume_thread_summary_request(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            1,
            "v1",
        )
        .await
        .expect("consume first version")
    );
    assert!(
        !crate::repositories::derived_events::consume_thread_summary_request(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            1,
            "v1",
        )
        .await
        .expect("repeat first version")
    );

    request.records[0] = terminal_record(format!("record-2-{}", Uuid::new_v4()).as_str());
    crate::repositories::records::batch_sync_records_with_partition_dispatch(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &request,
        true,
    )
    .await
    .expect("create second dispatch version");
    assert!(
        crate::repositories::derived_events::consume_thread_summary_request(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            2,
            "v1",
        )
        .await
        .expect("consume second version")
    );
    assert!(
        !crate::repositories::derived_events::consume_thread_summary_request(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            1,
            "v1",
        )
        .await
        .expect("ignore out-of-order old version")
    );
}

#[tokio::test]
async fn derived_event_dead_letter_rearm_preserves_history_and_advances_version() {
    let Some(pool) = pool().await else {
        return;
    };
    let (tenant, source, thread_id, _request) = setup(&pool).await;
    let event = claim(&pool, &tenant, &source, "publisher-1", 300).await;
    let status = crate::repositories::derived_events::mark_publish_failed(
        &pool,
        &event,
        "queue_unavailable",
        1,
        0,
    )
    .await
    .expect("dead-letter derived event");
    assert_eq!(status.as_deref(), Some("dead_letter"));

    let new_event_id = crate::repositories::derived_events::rearm_dead_lettered_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        1,
    )
    .await
    .expect("rearm dead-letter")
    .expect("new rearm event");
    assert_ne!(new_event_id, event.event_id);
    assert!(
        crate::repositories::derived_events::rearm_dead_lettered_thread_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            "project:alpha",
            1,
        )
        .await
        .expect("repeat rearm")
        .is_none()
    );

    let events: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT event_id,dispatch_version,status FROM engine_derived_event_outbox \
         WHERE tenant_id=$1 AND source_id=$2 ORDER BY dispatch_version",
    )
    .bind(&tenant)
    .bind(&source)
    .fetch_all(&pool)
    .await
    .expect("load dead-letter history");
    assert_eq!(
        events,
        vec![
            (event.event_id, 1, "dead_letter".to_string()),
            (new_event_id, 2, "pending".to_string())
        ]
    );
    let partition: (i64, Option<i64>, bool) = sqlx::query_as(
        "SELECT summary_dispatch_version,summary_dispatch_dead_letter_version,summary_dispatch_pending \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load rearmed partition");
    assert_eq!(partition, (2, None, true));
}
