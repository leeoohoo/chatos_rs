// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::UpsertRecordInput;
use crate::repositories::generation_commits::PrepareGeneration;

async fn create_thread(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    thread_id: &str,
    subject: &str,
) {
    crate::repositories::threads::upsert_thread(
        pool,
        thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
            subject_id: subject.to_string(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), subject.to_string()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create lifecycle thread");
}

fn record(record_id: &str) -> UpsertRecordInput {
    UpsertRecordInput {
        id: record_id.to_string(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: "lifecycle content".to_string(),
        structured_payload: None,
        metadata: Some(json!({
            "conversation_turn_id": format!("turn-{record_id}"),
            "memory_project_id": "project:alpha"
        })),
        summary_status: None,
        summary_id: None,
        summarized_at: None,
        created_at: "2026-11-09T00:00:00Z".to_string(),
    }
}

async fn ingest_record(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    thread_id: &str,
    record_id: &str,
) {
    crate::repositories::records::batch_sync_records(
        &test_config("unused".to_string()),
        pool,
        thread_id,
        &crate::models::BatchSyncRecordsRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
            project_partition_key: "project:alpha".to_string(),
            runtime_context_assertion: "repository-test".to_string(),
            records: vec![record(record_id)],
        },
    )
    .await
    .expect("ingest lifecycle record");
}

#[tokio::test]
async fn business_thread_delete_tombstones_rows_and_removes_them_from_recall() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let record_id = format!("record-{suffix}");
    create_thread(&pool, &tenant, &source, &thread_id, &subject).await;
    ingest_record(&pool, &tenant, &source, &thread_id, &record_id).await;

    let response = crate::repositories::threads::delete_thread(&pool, &tenant, &source, &thread_id)
        .await
        .expect("tombstone lifecycle thread");
    assert!(response.deleted_thread);
    assert_eq!(response.deleted_records, 1);
    assert_eq!(response.deleted_snapshots, 0);

    let thread_state: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status,recall_eligible,retired_reason FROM engine_threads \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load tombstoned thread");
    assert_eq!(
        thread_state,
        (
            "retired".to_string(),
            false,
            Some("business_delete".to_string())
        )
    );
    let record_state: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT summary_status,recall_eligible,retired_reason FROM engine_records \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("load tombstoned record");
    assert_eq!(
        record_state,
        (
            "retired".to_string(),
            false,
            Some("thread_retired".to_string())
        )
    );
    assert!(
        crate::repositories::threads::get_thread_by_id(&pool, &tenant, &source, &thread_id,)
            .await
            .expect("query retired thread")
            .is_none()
    );
    assert!(crate::repositories::records::get_record_by_id(
        &pool,
        &record_id,
        &tenant,
        &source,
        Some(&thread_id),
    )
    .await
    .expect("query retired record")
    .is_none());
}

#[tokio::test]
async fn privileged_erasure_removes_thread_content_lineage_and_generation_state() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let record_id = format!("record-{suffix}");
    let summary_id = format!("summary-{suffix}");
    let job_id = format!("job-{suffix}");
    create_thread(&pool, &tenant, &source, &thread_id, &subject).await;
    ingest_record(&pool, &tenant, &source, &thread_id, &record_id).await;
    sqlx::query(
        "INSERT INTO engine_job_runs(id,job_type,trigger_type,tenant_id,source_id,thread_id, \
         subject_id,status,started_at,data) \
         VALUES($1,'summary','database_worker',$2,$3,$4,$5,'running',now(),'{}'::jsonb)",
    )
    .bind(&job_id)
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(&subject)
    .execute(&pool)
    .await
    .expect("create erasure generation job");
    let generation = crate::repositories::generation_commits::prepare_generation(
        &pool,
        PrepareGeneration {
            tenant_id: &tenant,
            source_id: &source,
            job_run_id: &job_id,
            generation_step_key: "thread-l0:project:alpha:0",
            generation_kind: "thread_summary",
            input_digest: format!("sha256:{}", "a".repeat(64)).as_str(),
            input_manifest: &json!({
                "schema_version":"memory_generation_input_v1",
                "digest_algorithm":"sha256",
                "canonicalizer_revision":"canonical-json-v1",
                "inputs":[{
                    "kind":"record",
                    "id":record_id,
                    "ordinal":0,
                    "project_partition_key":"project:alpha",
                    "digest":format!("sha256:{}", "b".repeat(64))
                }]
            }),
            config_snapshot: &json!({
                "generation_profile":"thread_summary_v2",
                "prompt_checksum":format!("sha256:{}", "c".repeat(64)),
                "pipeline_revision":"pipeline-v1",
                "model_provider":"test-provider",
                "model_id":"test-model",
                "tokenizer_id":"test-tokenizer-v1",
                "policy_checksum":format!("sha256:{}", "d".repeat(64))
            }),
            project_partition_key: Some("project:alpha"),
        },
    )
    .await
    .expect("prepare erasure generation");
    crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &generation.generation_id,
        "erasure-fixture-worker",
        300,
    )
    .await
    .expect("claim erasure generation")
    .expect("erasure generation lease");
    let summary = EngineSummary {
        id: summary_id.clone(),
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        thread_id: thread_id.clone(),
        subject_id: subject.clone(),
        summary_type: "thread_incremental".to_string(),
        level: 0,
        source_digest: None,
        summary_text: "sensitive summary".to_string(),
        source_record_start_id: Some(record_id.clone()),
        source_record_end_id: Some(record_id.clone()),
        source_record_count: 1,
        status: "done".to_string(),
        rollup_status: "pending".to_string(),
        rollup_summary_id: None,
        rolled_up_at: None,
        subject_memory_summarized: 0,
        subject_memory_summarized_at: None,
        metadata: Some(json!({"project_subject_id":"project:alpha"})),
        payload_schema: None,
        structured_payload: None,
        created_at: "2026-11-09T00:00:01Z".to_string(),
        updated_at: "2026-11-09T00:00:01Z".to_string(),
    };
    let mut tx = pool
        .begin()
        .await
        .expect("begin erasure fixture transaction");
    sqlx::query(
        "UPDATE engine_generation_commits SET status='committed',output_kind='thread_summary', \
         output_id=$4,committed_at=now(),lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&generation.generation_id)
    .bind(&summary_id)
    .execute(&mut *tx)
    .await
    .expect("commit erasure generation registry");
    sqlx::query(
        "INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type, \
         level,status,rollup_status,created_at,updated_at,data,generation_id,generation_reason, \
         generation_profile,project_partition_key,recall_eligible,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,'thread_incremental',0,'done','pending',$6,$6,$7,$8, \
                'threshold','thread_summary_v2','project:alpha',true,'pipeline-v1')",
    )
    .bind(&summary_id)
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(&subject)
    .bind(timestamp(&summary.created_at).expect("summary timestamp"))
    .bind(json(&summary).expect("summary json"))
    .bind(&generation.generation_id)
    .execute(&mut *tx)
    .await
    .expect("insert erasure summary");
    sqlx::query(
        "INSERT INTO engine_summary_record_coverage(id,tenant_id,source_id,thread_id, \
         project_partition_key,summary_id,record_id,ordinal,source_ref,coverage_kind,status,generation_id) \
         VALUES($1,$2,$3,$4,'project:alpha',$5,$6,0,$6,'full','active',$7)",
    )
    .bind(format!("coverage-{suffix}"))
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(&summary_id)
    .bind(&record_id)
    .bind(&generation.generation_id)
    .execute(&mut *tx)
    .await
    .expect("insert erasure coverage");
    tx.commit()
        .await
        .expect("commit erasure fixture transaction");

    let erased = crate::repositories::erasure::erase_thread(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "legal_erasure",
    )
    .await
    .expect("privileged thread erasure");
    assert!(erased.deleted_thread);
    assert_eq!(erased.deleted_records, 1);
    assert_eq!(erased.deleted_summaries, 1);
    assert_eq!(erased.deleted_coverage, 1);
    assert_eq!(erased.deleted_generations, 1);

    for table in [
        "engine_threads",
        "engine_records",
        "engine_summaries",
        "engine_summary_record_coverage",
        "engine_thread_summary_partitions",
        "engine_derived_event_outbox",
        "engine_generation_commits",
        "engine_job_runs",
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE tenant_id=$1 AND source_id=$2");
        let remaining: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(&tenant)
            .bind(&source)
            .fetch_one(&pool)
            .await
            .expect("count erasure residue");
        assert_eq!(remaining, 0, "residue in {table}");
    }
    let audit: (String, i64, i64, i64) = sqlx::query_as(
        "SELECT reason_code,deleted_records,deleted_summaries,deleted_thread \
         FROM engine_privileged_erasure_audits WHERE erasure_id=$1",
    )
    .bind(&erased.erasure_id)
    .fetch_one(&pool)
    .await
    .expect("load non-content erasure audit");
    assert_eq!(audit, ("legal_erasure".to_string(), 1, 1, 1));
}

#[tokio::test]
async fn subject_memory_delete_is_a_tombstone_and_is_not_recalled() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = format!("agent:{suffix}");
    let memory_key = format!("memory-{suffix}");
    let memory = crate::repositories::subject_memories::insert_test_subject_memory(
        &pool,
        &subject,
        &memory_key,
        UpsertSubjectMemoryRequest {
            id: None,
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            memory_type: "agent_memory".to_string(),
            text: "sensitive memory".to_string(),
            level: Some(0),
            source_digest: None,
            confidence: None,
            last_seen_at: None,
            metadata: None,
            rollup_status: None,
            rollup_memory_key: None,
            rolled_up_at: None,
            status: None,
            created_at: None,
            updated_at: None,
        },
    )
    .await
    .expect("create subject memory for retirement");

    let retired = crate::repositories::subject_memories::retire_subject_memory(
        &pool,
        &tenant,
        &source,
        &subject,
        &memory_key,
    )
    .await
    .expect("retire subject memory");
    assert_eq!(retired, 1);
    let state: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status,recall_eligible,retired_reason FROM engine_subject_memories \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&memory.id)
    .fetch_one(&pool)
    .await
    .expect("load retired subject memory");
    assert_eq!(
        state,
        (
            "retired".to_string(),
            false,
            Some("business_delete".to_string())
        )
    );
    let recalled = crate::repositories::subject_memories::list_subject_memories(
        &pool, &tenant, &source, &subject, None, None, 100, 0,
    )
    .await
    .expect("query retired subject memory");
    assert!(recalled.is_empty());
}

#[tokio::test]
async fn restore_pending_repairs_only_records_without_active_coverage_and_requeues_partition() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let record_id = format!("record-{suffix}");
    create_thread(&pool, &tenant, &source, &thread_id, &subject).await;
    ingest_record(&pool, &tenant, &source, &thread_id, &record_id).await;
    let data = sqlx::query_scalar::<_, sqlx::types::Json<serde_json::Value>>(
        "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    ).bind(&tenant).bind(&source).bind(&record_id).fetch_one(&pool).await
        .expect("load restore-pending fixture record");
    let mut stored: crate::models::EngineRecord = crate::repositories::postgres::decode(data)
        .expect("decode restore-pending fixture record");
    stored.summary_status = "summarized".to_string();
    stored.summary_id = Some(format!("missing-summary-{suffix}"));
    stored.summarized_at = Some("2026-11-09T00:01:00Z".to_string());
    sqlx::query(
        "UPDATE engine_records SET summary_status='summarized',summary_id=$4,data=$5 \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    ).bind(&tenant).bind(&source).bind(&record_id).bind(stored.summary_id.as_deref())
        .bind(json(&stored).expect("encode restore-pending fixture"))
        .execute(&pool).await.expect("break record coverage fixture");
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET pending_record_count=0,pending_summary_tokens=0, \
         summary_status='idle',summary_dispatch_pending=false,summary_dispatch_consumed_version=summary_dispatch_version \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key='project:alpha'",
    ).bind(&tenant).bind(&source).bind(&thread_id).execute(&pool).await
        .expect("settle restore-pending fixture partition");

    let restored = crate::repositories::records::restore_record_pending(
        &pool, &tenant, &source, &record_id,
    ).await.expect("restore uncovered record pending");
    assert!(restored.changed);
    assert_eq!(restored.previous_status, "summarized");
    let state: (String, Option<String>, i64, bool) = sqlx::query_as(
        "SELECT record.summary_status,record.summary_id,partition.pending_record_count, \
         partition.summary_dispatch_pending FROM engine_records record \
         JOIN engine_thread_summary_partitions partition ON partition.tenant_id=record.tenant_id \
          AND partition.source_id=record.source_id AND partition.thread_id=record.thread_id \
          AND partition.project_partition_key=record.project_partition_key \
         WHERE record.tenant_id=$1 AND record.source_id=$2 AND record.id=$3",
    ).bind(&tenant).bind(&source).bind(&record_id).fetch_one(&pool).await
        .expect("load restored pending state");
    assert_eq!(state, ("pending".to_string(), None, 1, true));
}
