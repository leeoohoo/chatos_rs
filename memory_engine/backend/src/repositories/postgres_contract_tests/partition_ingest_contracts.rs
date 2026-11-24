// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::UpsertRecordInput;

fn record(id: &str, project: Option<&str>, terminal: bool) -> UpsertRecordInput {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "conversation_turn_id".to_string(),
        json!(format!("turn-{id}")),
    );
    if let Some(project) = project {
        metadata.insert("memory_project_id".to_string(), json!(project));
    }
    if terminal {
        metadata.insert("memory_terminal_summary_requested".to_string(), json!(true));
    }
    UpsertRecordInput {
        id: id.to_string(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: format!("content-{id}"),
        structured_payload: None,
        metadata: Some(serde_json::Value::Object(metadata)),
        summary_status: None,
        summary_id: None,
        summarized_at: None,
        created_at: "2026-11-09T00:00:00Z".to_string(),
    }
}

async fn thread(pool: &crate::db::Db, tenant: &str, source: &str, thread_id: &str) {
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
    .expect("create partition ingest thread");
}

#[tokio::test]
async fn partition_ingest_commits_record_counts_forced_dispatch_and_outbox_once() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    thread(&pool, &tenant, &source, &thread_id).await;
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "project:alpha".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![record(
            format!("record-{suffix}").as_str(),
            Some("alpha"),
            true,
        )],
    };
    let config = test_config("unused".to_string());

    let inserted =
        crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
            .await
            .expect("commit v2 partition ingest");
    assert_eq!(inserted, 1);

    let record_partition: Option<String> = sqlx::query_scalar(
        "SELECT project_partition_key FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&request.records[0].id)
    .fetch_one(&pool)
    .await
    .expect("load typed record partition");
    assert_eq!(record_partition.as_deref(), Some("project:alpha"));

    let partition: (i64, i64, i64, i64, bool) = sqlx::query_as(
        "SELECT pending_record_count,pending_summary_tokens,summary_dispatch_version,summary_dispatch_force_version,summary_dispatch_pending \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load partition state");
    assert_eq!(partition.0, 1);
    assert!(partition.1 > 0);
    assert_eq!(partition.2, 1);
    assert_eq!(partition.3, 1);
    assert!(partition.4);

    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_derived_event_outbox WHERE tenant_id=$1 AND source_id=$2 \
         AND aggregate_kind='thread_partition' AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .fetch_one(&pool)
    .await
    .expect("count derived events");
    assert_eq!(events, 1);

    let replayed =
        crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
            .await
            .expect("replay v2 partition ingest");
    assert_eq!(replayed, 0);
    let replay_state: (i64, i64, i64) = sqlx::query_as(
        "SELECT pending_record_count,summary_dispatch_version,summary_dispatch_force_version \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load replay state");
    assert_eq!(replay_state, (1, 1, 1));
    let replay_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_derived_event_outbox WHERE tenant_id=$1 AND source_id=$2 \
         AND aggregate_kind='thread_partition' AND project_partition_key='project:alpha'",
    )
    .bind(&tenant)
    .bind(&source)
    .fetch_one(&pool)
    .await
    .expect("count replayed derived events");
    assert_eq!(replay_events, 1);
}

#[tokio::test]
async fn partition_ingest_rejects_mixed_batches_before_writing() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    thread(&pool, &tenant, &source, &thread_id).await;
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "project:alpha".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![
            record(format!("record-a-{suffix}").as_str(), Some("alpha"), false),
            record(format!("record-b-{suffix}").as_str(), Some("beta"), false),
        ],
    };
    let error = crate::repositories::records::batch_sync_records(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &request,
    )
    .await
    .expect_err("mixed partition batch must fail");
    assert!(error.contains("does not match verified partition"));
    let records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("count rejected records");
    assert_eq!(records, 0);
}

#[tokio::test]
async fn partition_ingest_rejects_replay_that_changes_immutable_identity() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let record_id = format!("record-{suffix}");
    thread(&pool, &tenant, &source, &thread_id).await;
    let mut request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "project:alpha".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![record(&record_id, Some("alpha"), false)],
    };
    let config = test_config("unused".to_string());
    crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
        .await
        .expect("commit initial partition identity");

    request.records[0] = record(&record_id, Some("beta"), false);
    request.project_partition_key = "project:beta".to_string();
    let partition_error =
        crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
            .await
            .expect_err("partition-changing replay must fail");
    assert!(partition_error.contains("cannot change project_partition_key"));

    request.records[0] = record(&record_id, Some("alpha"), false);
    request.project_partition_key = "project:alpha".to_string();
    request.records[0].content = "changed-content".to_string();
    let content_error =
        crate::repositories::records::batch_sync_records(&config, &pool, &thread_id, &request)
            .await
            .expect_err("content-changing replay must fail");
    assert!(content_error.contains("content identity"));

    let stored: (String, i64) = sqlx::query_as(
        "SELECT project_partition_key,COUNT(*) OVER() FROM engine_records \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("load immutable stored record");
    assert_eq!(stored, ("project:alpha".to_string(), 1));
}

#[tokio::test]
async fn partition_ingest_normalizes_missing_project_to_none_partition() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let record_id = format!("record-{suffix}");
    thread(&pool, &tenant, &source, &thread_id).await;
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "__none__".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![record(&record_id, None, false)],
    };
    crate::repositories::records::batch_sync_records(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &request,
    )
    .await
    .expect("commit no-project record");

    let partitions: (String, String) = sqlx::query_as(
        "SELECT r.project_partition_key,p.project_partition_key FROM engine_records r \
         JOIN engine_thread_summary_partitions p USING(tenant_id,source_id,thread_id) \
         WHERE r.tenant_id=$1 AND r.source_id=$2 AND r.id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("load no-project partitions");
    assert_eq!(partitions, ("__none__".to_string(), "__none__".to_string()));
}

#[tokio::test]
async fn partition_ingest_rolls_back_record_and_thread_projection_on_partition_failure() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let record_id = format!("record-{suffix}");
    thread(&pool, &tenant, &source, &thread_id).await;
    sqlx::query(
        "INSERT INTO engine_thread_summary_partitions(tenant_id,source_id,thread_id,project_partition_key,summary_status,pending_record_count) \
         VALUES($1,$2,$3,'project:alpha','pending',$4)",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(i64::MAX)
    .execute(&pool)
    .await
    .expect("seed partition overflow guard");
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "project:alpha".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![record(&record_id, Some("alpha"), false)],
    };

    let error = crate::repositories::records::batch_sync_records(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &request,
    )
    .await
    .expect_err("partition arithmetic failure must abort ingest");
    assert!(error.contains("out of range"));

    let stored_records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("count rolled back record");
    assert_eq!(stored_records, 0);
}

#[tokio::test]
async fn partition_ingest_atomically_creates_thread_and_rejects_identity_changes() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let record_id = format!("record-{suffix}");
    let subject = format!("agent:{suffix}");
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "__none__".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![record(&record_id, None, false)],
    };
    let thread_request = |subject_id: &str| UpsertThreadRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        subject_id: subject_id.to_string(),
        thread_type: "conversation".to_string(),
        external_thread_id: Some(thread_id.clone()),
        title: None,
        labels: Some(vec!["memory-schema:v2".to_string(), subject_id.to_string()]),
        metadata: None,
        status: Some("active".to_string()),
        created_at: None,
        updated_at: None,
        archived_at: None,
    };

    let mut invalid_request = request.clone();
    invalid_request.records[0].created_at = "not-a-timestamp".to_string();
    crate::repositories::records::batch_sync_records_with_thread(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &invalid_request,
        Some(thread_request(&subject)),
    )
    .await
    .expect_err("record failure must roll back the new thread");
    let rolled_back_thread: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("count rolled back thread");
    assert_eq!(rolled_back_thread, 0);

    let inserted = crate::repositories::records::batch_sync_records_with_thread(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &request,
        Some(thread_request(&subject)),
    )
    .await
    .expect("create thread and record in one ingest");
    assert_eq!(inserted, 1);

    let mut archived_thread = thread_request(&subject);
    archived_thread.status = Some("archived".to_string());
    crate::repositories::threads::upsert_thread(&pool, &thread_id, archived_thread)
        .await
        .expect("archive established thread");
    let mut archived_ingest = request.clone();
    archived_ingest.records[0].id = format!("archived-{record_id}");
    crate::repositories::records::batch_sync_records_with_thread(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &archived_ingest,
        Some(thread_request(&subject)),
    )
    .await
    .expect("record ingest may not reactivate archived thread");
    let stored_status: String = sqlx::query_scalar(
        "SELECT status FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load preserved thread status");
    assert_eq!(stored_status, "archived");

    let mut changed_request = request.clone();
    changed_request.records[0].id = format!("changed-{record_id}");
    let changed_subject = format!("agent:changed-{suffix}");
    let error = crate::repositories::records::batch_sync_records_with_thread(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &changed_request,
        Some(thread_request(&changed_subject)),
    )
    .await
    .expect_err("established thread identity must be immutable");
    assert!(error.contains("thread identity is immutable"));
    let changed_record_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&changed_request.records[0].id)
    .fetch_one(&pool)
    .await
    .expect("count rejected identity-change record");
    assert_eq!(changed_record_count, 0);
}

#[tokio::test]
async fn partition_ingest_rejects_a_thread_project_outside_the_verified_partition() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let record_id = format!("record-{suffix}");
    let subject = format!("agent:{suffix}");
    let request = crate::models::BatchSyncRecordsRequest {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        project_partition_key: "project:a".to_string(),
        runtime_context_assertion: "repository-test".to_string(),
        records: vec![record(&record_id, Some("a"), false)],
    };
    let error = crate::repositories::records::batch_sync_records_with_thread(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &request,
        Some(UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            thread_type: "conversation".to_string(),
            external_thread_id: Some(thread_id.clone()),
            title: None,
            labels: Some(vec![
                "memory-schema:v2".to_string(),
                subject,
                "project:b".to_string(),
            ]),
            metadata: None,
            status: Some("active".to_string()),
            created_at: None,
            updated_at: None,
            archived_at: None,
        }),
    )
    .await
    .expect_err("thread project must be bound to the verified record partition");
    assert!(error.contains("does not match verified partition"));

    let stored_threads: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("count rejected thread");
    let stored_records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("count rejected record");
    assert_eq!((stored_threads, stored_records), (0, 0));
}

#[tokio::test]
async fn partition_ingest_serializes_competing_first_thread_identities() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let ingest = |subject: String, record_id: String| {
        let pool = pool.clone();
        let tenant = tenant.clone();
        let source = source.clone();
        let thread_id = thread_id.clone();
        tokio::spawn(async move {
            let request = crate::models::BatchSyncRecordsRequest {
                tenant_id: tenant.clone(),
                source_id: source.clone(),
                project_partition_key: "__none__".to_string(),
                runtime_context_assertion: "repository-test".to_string(),
                records: vec![record(&record_id, None, false)],
            };
            crate::repositories::records::batch_sync_records_with_thread(
                &test_config("unused".to_string()),
                &pool,
                &thread_id,
                &request,
                Some(UpsertThreadRequest {
                    tenant_id: tenant,
                    source_id: source,
                    subject_id: subject.clone(),
                    thread_type: "conversation".to_string(),
                    external_thread_id: Some(thread_id.clone()),
                    title: None,
                    labels: Some(vec!["memory-schema:v2".to_string(), subject]),
                    metadata: None,
                    status: Some("active".to_string()),
                    created_at: None,
                    updated_at: None,
                    archived_at: None,
                }),
            )
            .await
        })
    };
    let first = ingest(
        format!("agent:first-{suffix}"),
        format!("record-first-{suffix}"),
    );
    let second = ingest(
        format!("agent:second-{suffix}"),
        format!("record-second-{suffix}"),
    );
    let outcomes = [
        first.await.expect("first ingest task"),
        second.await.expect("second ingest task"),
    ];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter_map(|result| result.as_ref().err())
            .filter(|error| error.contains("thread identity is immutable"))
            .count(),
        1,
    );
    let stored_records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("count records after competing first ingests");
    assert_eq!(stored_records, 1);
}
