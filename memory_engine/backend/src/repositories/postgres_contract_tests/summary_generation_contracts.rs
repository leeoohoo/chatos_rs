// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::repositories::generation_commits::{GenerationCommit, PrepareGeneration};
use crate::repositories::generation_output_commits::ThreadSummaryGenerationOutput;

const PARTITION: &str = "project:alpha";
const PROFILE: &str = "thread_summary_v2";
const PIPELINE: &str = "pipeline-v1";

fn manifest(record_id: &str) -> serde_json::Value {
    json!({
        "schema_version": "memory_generation_input_v1",
        "digest_algorithm": "sha256",
        "canonicalizer_revision": "canonical-json-v1",
        "inputs": [{
            "kind": "record",
            "id": record_id,
            "ordinal": 0,
            "project_partition_key": PARTITION,
            "digest": format!("sha256:{}", "b".repeat(64))
        }]
    })
}

fn config(profile: &str) -> serde_json::Value {
    json!({
        "generation_profile": profile,
        "prompt_checksum": format!("sha256:{}", "c".repeat(64)),
        "pipeline_revision": PIPELINE,
        "model_provider": "test-provider",
        "model_id": "test-model",
        "tokenizer_id": "test-tokenizer-v1",
        "policy_checksum": format!("sha256:{}", "d".repeat(64))
    })
}

async fn thread(pool: &crate::db::Db, tenant: &str, source: &str, id: &str, subject: &str) {
    crate::repositories::threads::upsert_thread(
        pool,
        id,
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
    .expect("create summary generation thread");
}

async fn claimed_generation(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    thread_id: &str,
    subject: &str,
    suffix: &str,
    kind: &str,
    profile: &str,
) -> GenerationCommit {
    let job_id = format!("summary-generation-job-{suffix}");
    sqlx::query(
        "INSERT INTO engine_job_runs(id,job_type,trigger_type,tenant_id,source_id,thread_id, \
         subject_id,status,started_at,data) \
         VALUES($1,'summary','database_worker',$2,$3,$4,$5,'running',now(),'{}'::jsonb)",
    )
    .bind(&job_id)
    .bind(tenant)
    .bind(source)
    .bind(thread_id)
    .bind(subject)
    .execute(pool)
    .await
    .expect("create summary generation job");
    let input_manifest = manifest(format!("record-{suffix}").as_str());
    let config_snapshot = config(profile);
    let input_digest = format!("sha256:{}", "a".repeat(64));
    let prepared = crate::repositories::generation_commits::prepare_generation(
        pool,
        PrepareGeneration {
            tenant_id: tenant,
            source_id: source,
            job_run_id: &job_id,
            generation_step_key: if kind == "thread_summary" {
                "thread-l0:project:alpha:0"
            } else {
                "thread-rollup:project:alpha:1"
            },
            generation_kind: kind,
            input_digest: &input_digest,
            input_manifest: &input_manifest,
            config_snapshot: &config_snapshot,
            project_partition_key: Some(PARTITION),
        },
    )
    .await
    .expect("prepare summary generation");
    crate::repositories::generation_commits::claim_generation(
        pool,
        tenant,
        source,
        &prepared.generation_id,
        "summary-generation-worker",
        300,
    )
    .await
    .expect("claim summary generation")
    .expect("summary generation claim")
}

fn output<'a>(
    thread_id: &'a str,
    subject: &'a str,
    level: i64,
    profile: &'a str,
    text: &'a str,
) -> ThreadSummaryGenerationOutput<'a> {
    ThreadSummaryGenerationOutput {
        thread_id,
        subject_id: subject,
        summary_type: "thread_incremental",
        level,
        source_digest: Some(format!("sha256:{}", "e".repeat(64))),
        summary_text: text,
        source_record_start_id: Some("record-start".to_string()),
        source_record_end_id: Some("record-end".to_string()),
        source_record_count: 2,
        metadata: Some(json!({"generator":"memory_engine_summary_v2"})),
        generation_reason: "threshold",
        generation_profile: profile,
        project_partition_key: PARTITION,
        payload_schema: Some("thread_summary_v2"),
        structured_payload: None,
        pipeline_revision: PIPELINE,
    }
}

#[tokio::test]
async fn thread_summary_generation_commit_is_idempotent_and_persists_frozen_identity() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    thread(&pool, &tenant, &source, &thread_id, &subject).await;
    let generation = claimed_generation(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject,
        &suffix,
        "thread_summary",
        PROFILE,
    )
    .await;

    let summary = crate::repositories::generation_output_commits::commit_thread_summary_generation(
        &pool,
        &generation,
        output(&thread_id, &subject, 0, PROFILE, "first summary"),
    )
    .await
    .expect("commit generated thread summary");
    let replay = crate::repositories::generation_output_commits::commit_thread_summary_generation(
        &pool,
        &generation,
        output(&thread_id, &subject, 0, PROFILE, "ignored replay"),
    )
    .await
    .expect("replay generated thread summary callback");
    assert_eq!(replay.id, summary.id);
    assert_eq!(replay.summary_text, "first summary");

    let stored = sqlx::query_as::<_, (String, String, String, String, bool, i64, i64)>(
        "SELECT generation_id,generation_reason,generation_profile,pipeline_revision, \
         recall_eligible,rollup_dispatch_version,subject_memory_source_dispatch_version \
         FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&summary.id)
    .fetch_one(&pool)
    .await
    .expect("load generated summary identity");
    assert_eq!(stored.0, generation.generation_id);
    assert_eq!(stored.1, "threshold");
    assert_eq!(stored.2, PROFILE);
    assert_eq!(stored.3, PIPELINE);
    assert!(stored.4);
    assert_eq!((stored.5, stored.6), (1, 1));
    let downstream = sqlx::query_as::<_, (String, sqlx::types::Json<serde_json::Value>)>(
        "SELECT event_kind,payload FROM engine_derived_event_outbox \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3 ORDER BY event_kind",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&generation.generation_id)
    .fetch_all(&pool)
    .await
    .expect("load summary downstream events");
    assert_eq!(downstream.len(), 2);
    assert_eq!(downstream[0].0, "subject_memory_source_requested");
    assert_eq!(downstream[1].0, "thread_rollup_requested");
    assert!(downstream
        .iter()
        .all(|(_, payload)| payload.get("summary_text").is_none()));
    let claimed = crate::repositories::derived_events::claim_publish_batch_for_scope(
        &pool,
        "summary-output-publisher",
        300,
        10,
        Some(&tenant),
        Some(&source),
    )
    .await
    .expect("claim summary downstream events");
    assert_eq!(claimed.len(), 2);
    for event in claimed {
        assert!(
            crate::repositories::derived_events::mark_published(&pool, &event)
                .await
                .expect("publish summary downstream event")
        );
    }
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM engine_summaries \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&generation.generation_id)
    .fetch_one(&pool)
    .await
    .expect("count idempotent generated summary");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn summary_generation_scope_constraint_rolls_back_wrong_thread_then_allows_retry() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = format!("agent:{suffix}");
    let expected_thread = format!("thread-expected-{suffix}");
    let wrong_thread = format!("thread-wrong-{suffix}");
    thread(&pool, &tenant, &source, &expected_thread, &subject).await;
    thread(&pool, &tenant, &source, &wrong_thread, &subject).await;
    let generation = claimed_generation(
        &pool,
        &tenant,
        &source,
        &expected_thread,
        &subject,
        &suffix,
        "thread_summary",
        PROFILE,
    )
    .await;

    let error = crate::repositories::generation_output_commits::commit_thread_summary_generation(
        &pool,
        &generation,
        output(&wrong_thread, &subject, 0, PROFILE, "wrong thread"),
    )
    .await
    .expect_err("database rejects summary output outside job thread");
    assert!(error.contains("output thread does not match job scope"));
    let output_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM engine_summaries \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&generation.generation_id)
    .fetch_one(&pool)
    .await
    .expect("count rolled back wrong-thread output");
    assert_eq!(output_count, 0);
    let current = crate::repositories::generation_commits::get_generation(
        &pool,
        &tenant,
        &source,
        &generation.generation_id,
    )
    .await
    .expect("load generation after scope rollback")
    .expect("generation remains after scope rollback");
    assert_eq!(current.status, "running");

    let summary = crate::repositories::generation_output_commits::commit_thread_summary_generation(
        &pool,
        &generation,
        output(&expected_thread, &subject, 0, PROFILE, "correct thread"),
    )
    .await
    .expect("retry summary commit after scope rollback");
    assert_eq!(summary.thread_id, expected_thread);
}

#[tokio::test]
async fn thread_rollup_generation_requires_rollup_kind_and_matching_frozen_config() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    thread(&pool, &tenant, &source, &thread_id, &subject).await;
    let rollup_profile = "thread_rollup_v2";
    let generation = claimed_generation(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject,
        &suffix,
        "thread_rollup",
        rollup_profile,
    )
    .await;
    let config_error =
        crate::repositories::generation_output_commits::commit_thread_summary_generation(
            &pool,
            &generation,
            output(&thread_id, &subject, 1, PROFILE, "rollup"),
        )
        .await
        .expect_err("rollup callback cannot change frozen profile");
    assert!(config_error.contains("frozen generation"));
    let rollup = crate::repositories::generation_output_commits::commit_thread_summary_generation(
        &pool,
        &generation,
        output(&thread_id, &subject, 1, rollup_profile, "rollup"),
    )
    .await
    .expect("commit rollup generation");
    assert_eq!(rollup.level, 1);
}

#[tokio::test]
async fn thread_l0_final_commit_atomically_finishes_records_partition_projection_and_job() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let record_id = format!("record-{suffix}");
    thread(&pool, &tenant, &source, &thread_id, &subject).await;
    crate::repositories::records::batch_sync_records_with_partition_dispatch(
        &test_config("unused".to_string()),
        &pool,
        &thread_id,
        &crate::models::BatchSyncRecordsRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            records: vec![crate::models::UpsertRecordInput {
                id: record_id.clone(),
                external_record_id: None,
                role: "user".to_string(),
                record_type: "message".to_string(),
                content: "atomic source".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "conversation_turn_id": format!("turn-{suffix}"),
                    "memory_project_id": "alpha"
                })),
                summary_status: None,
                summary_id: None,
                summarized_at: None,
                created_at: "2026-11-09T00:00:00Z".to_string(),
            }],
        },
        true,
    )
    .await
    .expect("ingest L0 source record");
    let pending = sqlx::query_as::<_, (i64, i64)>(
        "SELECT pending_record_count,pending_summary_tokens \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND project_partition_key=$4",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(PARTITION)
    .fetch_one(&pool)
    .await
    .expect("load pending L0 partition");
    assert_eq!(pending.0, 1);
    assert!(crate::repositories::thread_partitions::mark_ready(
        &pool, &tenant, &source, &thread_id, PARTITION, pending.0, pending.1,
    )
    .await
    .expect("mark L0 partition ready"));
    assert!(crate::repositories::thread_partitions::cutover_to_v2(
        &pool, &tenant, &source, &thread_id, PARTITION,
    )
    .await
    .expect("cut over L0 partition"));

    let job = crate::repositories::control_plane::create_job_run(
        &pool,
        crate::models::CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: "database_worker".to_string(),
            tenant_id: Some(tenant.clone()),
            source_id: Some(source.clone()),
            thread_id: Some(thread_id.clone()),
            subject_id: Some(subject.clone()),
            thread_label: None,
            metadata: Some(json!({"phase":"running"})),
        },
    )
    .await
    .expect("create atomic L0 job");
    crate::repositories::thread_partitions::try_claim_summary(
        &pool, &tenant, &source, &thread_id, PARTITION, "v2", &job.id, 300,
    )
    .await
    .expect("claim v2 L0 partition")
    .expect("v2 L0 partition claim");
    assert_eq!(
        crate::repositories::records::claim_records_for_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&record_id),
            &job.id,
        )
        .await
        .expect("claim L0 source record"),
        1
    );

    let input_manifest = manifest(&record_id);
    let config_snapshot = config(PROFILE);
    let input_digest = format!("sha256:{}", "a".repeat(64));
    let prepared = crate::repositories::generation_commits::prepare_generation(
        &pool,
        PrepareGeneration {
            tenant_id: &tenant,
            source_id: &source,
            job_run_id: &job.id,
            generation_step_key: "thread-l0:project:alpha:0",
            generation_kind: "thread_summary",
            input_digest: &input_digest,
            input_manifest: &input_manifest,
            config_snapshot: &config_snapshot,
            project_partition_key: Some(PARTITION),
        },
    )
    .await
    .expect("prepare atomic L0 generation");
    let generation = crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &prepared.generation_id,
        "atomic-L0-worker",
        300,
    )
    .await
    .expect("claim atomic L0 generation")
    .expect("atomic L0 generation claim");
    let mut summary_output = output(&thread_id, &subject, 0, PROFILE, "atomic L0 summary");
    summary_output.source_record_start_id = Some(record_id.clone());
    summary_output.source_record_end_id = Some(record_id.clone());
    summary_output.source_record_count = 1;
    let summary =
        crate::repositories::generation_output_commits::commit_thread_summary_generation_final(
            &pool,
            &generation,
            summary_output,
            crate::repositories::generation_output_commits::ThreadL0GenerationFinalization {
                record_ids: std::slice::from_ref(&record_id),
                consumed_summary_tokens: pending.1,
                job_result: crate::models::FinishEngineJobRunRequest {
                    status: "done".to_string(),
                    input_count: 1,
                    output_count: 1,
                    processed_count: 1,
                    success_count: 1,
                    error_count: 0,
                    metadata: Some(json!({"summary_id":"committed"})),
                    error_message: None,
                },
            },
        )
        .await
        .expect("atomically commit L0 generation");

    let record_state = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT summary_status,summary_id,summary_job_run_id FROM engine_records \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&record_id)
    .fetch_one(&pool)
    .await
    .expect("load committed L0 source record");
    assert_eq!(record_state.0, "summarized");
    assert_eq!(record_state.1.as_deref(), Some(summary.id.as_str()));
    assert!(record_state.2.is_none());
    let partition_state = sqlx::query_as::<_, (i64, i64, String, Option<String>)>(
        "SELECT pending_record_count,pending_summary_tokens,summary_status,summary_job_run_id \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND project_partition_key=$4",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(PARTITION)
    .fetch_one(&pool)
    .await
    .expect("load finalized L0 partition");
    assert_eq!(partition_state, (0, 0, "idle".to_string(), None));
    let thread_state = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT pending_record_count,pending_summary_tokens,summary_status FROM engine_threads \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .fetch_one(&pool)
    .await
    .expect("load finalized thread projection");
    assert_eq!(thread_state, (0, 0, "idle".to_string()));
    let job_state = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT status,(data->>'success_count')::bigint,(data->>'output_count')::bigint \
         FROM engine_job_runs WHERE id=$1",
    )
    .bind(&job.id)
    .fetch_one(&pool)
    .await
    .expect("load finalized L0 job");
    assert_eq!(job_state, ("done".to_string(), 1, 1));
}
