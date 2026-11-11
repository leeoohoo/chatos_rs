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
        project_partition_key: "__none__".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
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

    let state = sqlx::query_as::<_, (i64, i64, bool)>(
        "SELECT summary_dispatch_version,summary_dispatch_force_version,summary_dispatch_pending \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND project_partition_key='__none__'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load partition dispatch state");
    assert_eq!(state.0, 1);
    assert_eq!(state.1, state.0);
    assert!(state.2, "forced partition event is publishable");

    let outbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM engine_derived_event_outbox WHERE tenant_id=$1 AND source_id=$2 \
         AND event_kind='thread_summary_requested' AND aggregate_kind='thread_partition' \
         AND project_partition_key='__none__' AND dispatch_version=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(state.0)
    .fetch_one(&pool)
    .await
    .expect("load forced summary outbox event");
    assert_eq!(outbox_count, 1);
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
        project_partition_key: "__none__".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
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
