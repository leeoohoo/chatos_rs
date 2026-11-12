// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::models::{BatchSyncRecordsRequest, UpsertRecordInput, UpsertThreadRequest};

async fn pool() -> Option<crate::db::Db> {
    let url = std::env::var("MEMORY_ENGINE_DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

fn test_config() -> crate::config::AppConfig {
    crate::config::AppConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        database_url: "unused".to_string(),
        ai_request_timeout_secs: 5,
        api_enabled: true,
        worker_enabled: false,
        worker_interval_secs: 30,
        worker_max_threads_per_tick: 1,
        worker_summary_concurrency: 1,
        worker_rollup_concurrency: 1,
        worker_subject_memory_concurrency: 1,
        worker_reconcile_concurrency: 1,
        cloud_agent_outbox_reconcile_interval: Duration::from_secs(1),
        cloud_agent_outbox_batch_size: 10,
        summary_retry_delay: Duration::from_millis(100),
        summary_outbox_reconcile_interval: Duration::from_secs(1),
        summary_outbox_batch_size: 10,
        rollup_retry_delay: Duration::from_millis(100),
        rollup_outbox_reconcile_interval: Duration::from_secs(1),
        rollup_outbox_batch_size: 10,
        subject_memory_retry_delay: Duration::from_millis(100),
        subject_memory_outbox_reconcile_interval: Duration::from_secs(1),
        subject_memory_outbox_batch_size: 10,
        subject_memory_lock_timeout_secs: 300,
        record_sync_lease_timeout_secs: 300,
        rollup_lock_timeout_secs: 300,
        internal_api_secrets: HashMap::new(),
        require_signed_internal_requests: true,
        runtime_context_assertion_keys: crate::config::test_runtime_context_assertion_keys(),
        model_capabilities: crate::config::test_model_capabilities(),
        active_model_capability_revision: "test-model-capability-v1".into(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_internal_base_url: "https://127.0.0.1:39192".to_string(),
        user_service_internal_http: reqwest::Client::new(),
        user_service_request_timeout_ms: 300,
    }
}

async fn thread(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    thread_id: &str,
) -> crate::models::EngineThread {
    let subject = format!("agent:{thread_id}");
    crate::repositories::threads::upsert_thread(
        pool,
        thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
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
    .expect("create fail-closed thread")
}

#[tokio::test]
async fn oversized_thread_record_uses_splitter_without_pending_decrement_on_failure() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let thread = thread(&pool, &tenant, &source, &thread_id).await;
    let record_id = format!("record-{suffix}");
    let request = BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "__none__".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![UpsertRecordInput {
            id: record_id.clone(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: "x".repeat(20_000),
            structured_payload: None,
            metadata: Some(json!({"conversation_turn_id": format!("turn-{suffix}")})),
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: "2026-11-09T00:00:00Z".to_string(),
        }],
    };
    crate::repositories::records::batch_sync_records(&test_config(), &pool, &thread_id, &request)
        .await
        .expect("insert oversized pending record");
    let pending =
        crate::repositories::records::list_pending_records(&pool, &tenant, &source, &thread_id, 10)
            .await
            .expect("load oversized pending record");
    let selection = select_pending_records_for_summary(pending, 500);
    assert_eq!(selection.selected.len(), 1);
    assert!(selection.oversized.is_empty());
    let ctx = ThreadSummaryExecutionContext {
        thread,
        settings: SummaryJobSettings {
            token_limit: 500,
            source_batch_token_limit: 500,
            cloud_owner_entity_id: None,
            cloud_resume_kind: None,
        },
        pending_before_count: 1,
        selection,
    };
    let job = start_thread_summary_job(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &ctx,
        THREAD_DIRECT_TRIGGER,
    )
    .await
    .expect("claim oversized summary job");
    let error = execute_prepared_thread_summary_job(
        &test_config(),
        &pool,
        &tenant,
        &source,
        &thread_id,
        &job.id,
        ctx,
    )
    .await
    .expect_err("oversized source must fail closed");
    assert!(!error.trim().is_empty());

    let record = crate::repositories::records::get_record_by_id(
        &pool,
        &record_id,
        &tenant,
        &source,
        Some(&thread_id),
    )
    .await
    .expect("load fail-closed record")
    .expect("record remains");
    assert_eq!(record.summary_status, "pending");
    assert!(record.summary_id.is_none());
    let state = sqlx::query_as::<_, (i64, String)>(
        "SELECT pending_record_count,summary_status FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='__none__'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load fail-closed partition");
    assert_eq!(state.0, 1);
    assert_ne!(state.1, "running");
}

#[tokio::test]
async fn oversized_thread_rollup_fails_without_marking_source_done() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let thread = thread(&pool, &tenant, &source, &thread_id).await;
    let summary = crate::repositories::summaries::insert_test_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &thread.subject_id,
        "x".repeat(20_000).as_str(),
        None,
        None,
        1,
        None,
    )
    .await
    .expect("create oversized source summary");
    let prepared = PreparedThreadRollup {
        thread_id: thread_id.clone(),
        project_partition_key: "__none__".to_string(),
        level: 0,
        selected: vec![summary.clone()],
        trigger_reason: "token_limit".to_string(),
        cloud_job_run_id: None,
    };
    let settings = RollupSettings {
        token_limit: 500,
        source_batch_token_limit: 500,
        cloud_owner_entity_id: None,
        cloud_source_id: None,
        cloud_thread_id: None,
    };
    let error = run_prepared_thread_rollup(
        &test_config(),
        &pool,
        &tenant,
        &source,
        &thread_id,
        prepared,
        &settings,
        SCHEDULER_TRIGGER,
    )
    .await
    .expect_err("oversized rollup source must fail closed");
    assert!(!error.trim().is_empty());
    let stored =
        crate::repositories::summaries::get_summary_by_id(&pool, &tenant, &source, &summary.id)
            .await
            .expect("reload oversized source summary")
            .expect("source summary remains");
    assert_eq!(stored.rollup_status, "pending");
    assert!(stored.rollup_summary_id.is_none());
}

#[tokio::test]
async fn late_success_callback_acknowledges_an_already_failed_summary_job() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let thread = thread(&pool, &tenant, &source, &thread_id).await;
    let job = crate::repositories::control_plane::create_job_run(
        &pool,
        crate::models::CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: "database_worker".to_string(),
            tenant_id: Some(tenant.clone()),
            source_id: Some(source.clone()),
            thread_id: Some(thread_id.clone()),
            subject_id: Some(thread.subject_id),
            thread_label: None,
            metadata: None,
        },
    )
    .await
    .expect("create summary job for late callback");
    crate::repositories::control_plane::finish_job_run(
        &pool,
        &job.id,
        crate::models::FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: 1,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: 1,
            metadata: None,
            error_message: Some("stale recovery released the claim".to_string()),
        },
    )
    .await
    .expect("fail summary job before callback");

    let outcome =
        resume_cloud_summary_job(&test_config(), &pool, &tenant, &source, &thread_id, &job.id)
            .await
            .expect("late terminal callback is acknowledged");
    assert!(outcome.is_none());
}
