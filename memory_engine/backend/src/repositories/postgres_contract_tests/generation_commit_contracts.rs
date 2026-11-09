// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::repositories::generation_commits::{GenerationCommit, PrepareGeneration};
use sqlx::types::Json;

fn manifest(record_id: &str) -> serde_json::Value {
    json!({
        "schema_version": "memory_generation_input_v1",
        "digest_algorithm": "sha256",
        "canonicalizer_revision": "canonical-json-v1",
        "inputs": [{
            "kind": "record",
            "id": record_id,
            "ordinal": 0,
            "project_partition_key": "project:alpha",
            "digest": format!("sha256:{}", "b".repeat(64))
        }]
    })
}

fn config(profile: &str) -> serde_json::Value {
    json!({
        "generation_profile": profile,
        "prompt_checksum": format!("sha256:{}", "c".repeat(64)),
        "pipeline_revision": "pipeline-v1",
        "model_provider": "test-provider",
        "model_id": "test-model",
        "tokenizer_id": "test-tokenizer-v1",
        "policy_checksum": format!("sha256:{}", "d".repeat(64))
    })
}

async fn job(pool: &crate::db::Db, tenant: &str, source: &str, id: &str) {
    sqlx::query(
        "INSERT INTO engine_job_runs(id,job_type,trigger_type,tenant_id,source_id,status,started_at,data) \
         VALUES($1,'summary','database_worker',$2,$3,'running',now(),'{}'::jsonb)",
    )
    .bind(id)
    .bind(tenant)
    .bind(source)
    .execute(pool)
    .await
    .expect("create generation job");
}

async fn prepare(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    job_id: &str,
    manifest: &serde_json::Value,
    config: &serde_json::Value,
) -> Result<GenerationCommit, String> {
    let input_digest = format!("sha256:{}", "a".repeat(64));
    crate::repositories::generation_commits::prepare_generation(
        pool,
        PrepareGeneration {
            tenant_id: tenant,
            source_id: source,
            job_run_id: job_id,
            generation_step_key: "thread-l0:project:alpha:0",
            generation_kind: "thread_summary",
            input_digest: input_digest.as_str(),
            input_manifest: manifest,
            config_snapshot: config,
            project_partition_key: Some("project:alpha"),
        },
    )
    .await
}

#[tokio::test]
async fn generation_prepare_reuses_exact_snapshot_and_rejects_drift_or_cross_scope_job() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let job_id = format!("job-{suffix}");
    job(&pool, &tenant, &source, &job_id).await;
    let input_manifest = manifest(format!("record-{suffix}").as_str());
    let config_snapshot = config("thread_summary_v2");
    let first = prepare(
        &pool,
        &tenant,
        &source,
        &job_id,
        &input_manifest,
        &config_snapshot,
    )
    .await
    .expect("prepare generation");
    let replay = prepare(
        &pool,
        &tenant,
        &source,
        &job_id,
        &input_manifest,
        &config_snapshot,
    )
    .await
    .expect("replay exact generation");
    assert_eq!(replay.generation_id, first.generation_id);

    let drift = prepare(
        &pool,
        &tenant,
        &source,
        &job_id,
        &input_manifest,
        &config("thread_summary_v2_changed"),
    )
    .await
    .expect_err("same job step cannot change configuration");
    assert!(drift.contains("changed frozen input or configuration"));

    let wrong_tenant = format!("other-{tenant}");
    let cross_scope = prepare(
        &pool,
        &wrong_tenant,
        &source,
        &job_id,
        &input_manifest,
        &config_snapshot,
    )
    .await
    .expect_err("generation cannot bind a cross-scope job");
    assert!(cross_scope.contains("generation job scope"));
}

#[tokio::test]
async fn generation_lease_takeover_and_failure_are_token_guarded() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let job_id = format!("job-{suffix}");
    job(&pool, &tenant, &source, &job_id).await;
    let generation = prepare(
        &pool,
        &tenant,
        &source,
        &job_id,
        &manifest("record-1"),
        &config("thread_summary_v2"),
    )
    .await
    .expect("prepare leased generation");
    let first = crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &generation.generation_id,
        "worker-1",
        300,
    )
    .await
    .expect("claim generation")
    .expect("first generation claim");
    assert!(crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &generation.generation_id,
        "worker-2",
        300,
    )
    .await
    .expect("active lease blocks claim")
    .is_none());
    sqlx::query(
        "UPDATE engine_generation_commits SET lease_expires_at=now()-interval '1 second' \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&generation.generation_id)
    .execute(&pool)
    .await
    .expect("expire generation lease");
    let takeover = crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &generation.generation_id,
        "worker-2",
        300,
    )
    .await
    .expect("take over generation")
    .expect("takeover claim");
    assert_eq!(takeover.attempt_count, 2);
    assert_ne!(takeover.lease_token, first.lease_token);
    assert!(
        !crate::repositories::generation_commits::mark_attempt_failed(
            &pool,
            &first,
            "stale_worker_failure",
        )
        .await
        .expect("stale failure CAS")
    );
    assert!(
        crate::repositories::generation_commits::mark_attempt_failed(
            &pool,
            &takeover,
            "model_unavailable",
        )
        .await
        .expect("current failure CAS")
    );
    let stored = crate::repositories::generation_commits::get_generation(
        &pool,
        &tenant,
        &source,
        &generation.generation_id,
    )
    .await
    .expect("load failed generation")
    .expect("generation remains");
    assert_eq!(stored.status, "prepared");
    assert_eq!(stored.attempt_count, 2);
    assert!(stored.lease_token.is_none());
}

#[tokio::test]
async fn generation_commit_requires_exactly_one_matching_output_and_current_lease() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), subject.clone()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create generation output thread");
    let job_id = format!("job-{suffix}");
    job(&pool, &tenant, &source, &job_id).await;
    let prepared = prepare(
        &pool,
        &tenant,
        &source,
        &job_id,
        &manifest("record-1"),
        &config("thread_summary_v2"),
    )
    .await
    .expect("prepare committed generation");
    let claimed = crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &prepared.generation_id,
        "worker-1",
        300,
    )
    .await
    .expect("claim committed generation")
    .expect("generation claim");
    let summary_id = format!("summary-{suffix}");
    let mut tx = pool.begin().await.expect("begin generation commit");
    sqlx::query(
        "INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type,level,status, \
         rollup_status,created_at,updated_at,data,generation_id,project_partition_key,generation_profile, \
         recall_eligible,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,'thread_incremental',0,'done','pending',now(),now(),$6,$7, \
         'project:alpha','thread_summary_v2',true,'pipeline-v1')",
    )
    .bind(&summary_id)
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(&subject)
    .bind(Json(json!({"id": summary_id, "summary_text": "generated"})))
    .bind(&prepared.generation_id)
    .execute(&mut *tx)
    .await
    .expect("insert generated summary");
    assert!(crate::repositories::generation_commits::commit_generation(
        &mut tx,
        &claimed,
        "thread_summary",
        &summary_id,
    )
    .await
    .expect("CAS generation commit"));
    tx.commit().await.expect("commit exact generation output");
    let stored = crate::repositories::generation_commits::get_generation(
        &pool,
        &tenant,
        &source,
        &prepared.generation_id,
    )
    .await
    .expect("load committed generation")
    .expect("committed generation");
    assert_eq!(stored.status, "committed");
    assert_eq!(stored.output_id.as_deref(), Some(summary_id.as_str()));

    let job_id_2 = format!("job-2-{suffix}");
    job(&pool, &tenant, &source, &job_id_2).await;
    let missing = prepare(
        &pool,
        &tenant,
        &source,
        &job_id_2,
        &manifest("record-2"),
        &config("thread_summary_v2"),
    )
    .await
    .expect("prepare missing-output generation");
    let missing_claim = crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &missing.generation_id,
        "worker-2",
        300,
    )
    .await
    .expect("claim missing-output generation")
    .expect("missing-output claim");
    let mut invalid_tx = pool.begin().await.expect("begin invalid commit");
    assert!(crate::repositories::generation_commits::commit_generation(
        &mut invalid_tx,
        &missing_claim,
        "thread_summary",
        "missing-summary",
    )
    .await
    .expect("stage invalid generation commit"));
    let error = invalid_tx
        .commit()
        .await
        .expect_err("constraint trigger rejects missing output");
    assert!(error
        .to_string()
        .contains("exactly one thread summary output"));

    let job_id_3 = format!("job-3-{suffix}");
    job(&pool, &tenant, &source, &job_id_3).await;
    let guarded = prepare(
        &pool,
        &tenant,
        &source,
        &job_id_3,
        &manifest("record-3"),
        &config("thread_summary_v2"),
    )
    .await
    .expect("prepare database-guarded generation");
    let direct_commit = sqlx::query(
        "UPDATE engine_generation_commits SET status='committed',output_kind='thread_summary',output_id='bypass' \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&guarded.generation_id)
    .execute(&pool)
    .await
    .expect_err("database rejects prepared-to-committed bypass");
    assert!(direct_commit
        .to_string()
        .contains("invalid generation state transition"));

    let orphan_id = format!("orphan-summary-{suffix}");
    let mut orphan_tx = pool.begin().await.expect("begin orphan output");
    sqlx::query(
        "INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type,level,status, \
         rollup_status,created_at,updated_at,data,generation_id,project_partition_key,generation_profile, \
         recall_eligible,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,'thread_incremental',0,'done','pending',now(),now(),$6,$7, \
         'project:alpha','thread_summary_v2',true,'pipeline-v1')",
    )
    .bind(&orphan_id)
    .bind(&tenant)
    .bind(&source)
    .bind(&thread_id)
    .bind(&subject)
    .bind(Json(json!({"id": orphan_id, "summary_text": "orphan"})))
    .bind(&guarded.generation_id)
    .execute(&mut *orphan_tx)
    .await
    .expect("stage orphan generated output");
    let orphan_error = orphan_tx
        .commit()
        .await
        .expect_err("database rejects output without committed registry");
    assert!(orphan_error
        .to_string()
        .contains("generated output must match a committed generation"));
}
