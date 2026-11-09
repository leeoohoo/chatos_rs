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
