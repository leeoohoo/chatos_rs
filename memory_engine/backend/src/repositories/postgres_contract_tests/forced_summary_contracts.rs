// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn terminal_record_requests_forced_summary_below_normal_threshold() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("client-agent:task:{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "task".to_string(),
            external_thread_id: Some(thread_id.clone()),
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), agent_subject_id]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert task thread");

    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        records: vec![crate::models::UpsertRecordInput {
            id: format!("record-{suffix}"),
            external_record_id: None,
            role: "assistant".to_string(),
            record_type: "message".to_string(),
            content: "done".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_turn_id": format!("turn-{suffix}"),
                "memory_terminal_summary_requested": true,
            })),
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: "2026-11-05T00:00:00Z".to_string(),
        }],
    };
    let config = test_config(
        std::env::var("MEMORY_ENGINE_DATABASE_URL").expect("database URL used by test pool"),
    );
    crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
        .await
        .expect("sync terminal record");

    let state = crate::repositories::threads::get_summary_dispatch_state(
        &pool, &tenant, &source, &thread_id,
    )
    .await
    .expect("load dispatch state")
    .expect("dispatch state exists");
    assert!(state.forced());
    assert_eq!(state.summary_dispatch_version, 1);

    let pending = sqlx::query_scalar::<_, bool>(
        "SELECT summary_dispatch_pending FROM engine_threads \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load pending dispatch flag");
    assert!(pending, "forced event is publishable");
    let event = state;
    assert!(event.forced());

    crate::repositories::threads::mark_summary_dispatch_consumed(&pool, &event)
        .await
        .expect("consume forced event");
    let rearmed = crate::repositories::threads::rearm_summary_dispatch_after_processing(
        &pool,
        &event,
        i64::MAX,
    )
    .await
    .expect("rearm forced event")
    .expect("pending terminal records keep forced dispatch armed");
    assert!(rearmed.forced());
}

#[tokio::test]
async fn stale_forced_summary_is_recovered_below_normal_threshold() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "task".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), agent_subject_id]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert task thread");
    crate::repositories::threads::apply_summary_queue_state_delta(
        &pool, &tenant, &source, &thread_id, 1, 1,
    )
    .await
    .expect("mark one pending record");
    let event = crate::repositories::threads::request_forced_summary_dispatch(
        &pool, &tenant, &source, &thread_id,
    )
    .await
    .expect("request forced dispatch")
    .expect("forced dispatch created");

    sqlx::query(
        "UPDATE engine_threads SET summary_dispatch_pending=false, \
         summary_dispatch_published_version=summary_dispatch_version, \
         summary_dispatch_published_at=now()-interval '1 hour' \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .execute(&pool)
    .await
    .expect("age published dispatch");

    let stale = crate::repositories::threads::list_stale_published_summary_dispatches(
        &pool,
        i64::MAX,
        "2999-01-01T00:00:00Z",
        10_000,
    )
    .await
    .expect("list stale forced dispatches");
    assert!(stale.iter().any(|candidate| {
        candidate.thread_id == thread_id && candidate.tenant_id == tenant && candidate.forced()
    }));

    let recovered = crate::repositories::threads::rearm_stale_published_summary_dispatch(
        &pool,
        &event,
        i64::MAX,
        "2999-01-01T00:00:00Z",
    )
    .await
    .expect("recover stale forced dispatch")
    .expect("forced dispatch is recoverable below the normal threshold");
    assert!(recovered.forced());
}

#[tokio::test]
async fn replay_preserves_memory_engine_summary_lifecycle() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("client-agent:task:{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    let record_id = format!("record-{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "task".to_string(),
            external_thread_id: Some(thread_id.clone()),
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), agent_subject_id]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert task thread");
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        records: vec![crate::models::UpsertRecordInput {
            id: record_id.clone(),
            external_record_id: None,
            role: "assistant".to_string(),
            record_type: "message".to_string(),
            content: "done".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_turn_id": format!("turn-{suffix}"),
                "memory_terminal_summary_requested": true,
            })),
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: "2026-11-05T00:00:00Z".to_string(),
        }],
    };
    let config = test_config(
        std::env::var("MEMORY_ENGINE_DATABASE_URL").expect("database URL used by test pool"),
    );
    crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
        .await
        .expect("initial record sync");
    crate::repositories::records::mark_records_summarized(
        &pool,
        &tenant,
        &source,
        &thread_id,
        std::slice::from_ref(&record_id),
        "summary-1",
    )
    .await
    .expect("mark record summarized");

    crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
        .await
        .expect("replay committed record sync");
    let replayed = crate::repositories::records::get_record_by_id(
        &pool,
        &record_id,
        &tenant,
        &source,
        Some(&thread_id),
    )
    .await
    .expect("load replayed record")
    .expect("replayed record exists");
    assert_eq!(replayed.summary_status, "summarized");
    assert_eq!(replayed.summary_id.as_deref(), Some("summary-1"));
    assert!(replayed.summarized_at.is_some());
}
