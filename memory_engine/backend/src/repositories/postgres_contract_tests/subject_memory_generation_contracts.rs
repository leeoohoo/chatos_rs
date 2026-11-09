// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::repositories::generation_commits::{GenerationCommit, PrepareGeneration};
use crate::repositories::generation_output_commits::SubjectMemoryGenerationOutput;

const PARTITION: &str = "project:alpha";
const PROFILE: &str = "project_memory_v2";
const PIPELINE: &str = "pipeline-v1";

fn manifest(record_id: &str) -> serde_json::Value {
    json!({
        "schema_version": "memory_generation_input_v1",
        "digest_algorithm": "sha256",
        "canonicalizer_revision": "canonical-json-v1",
        "inputs": [{
            "kind": "summary",
            "id": record_id,
            "ordinal": 0,
            "project_partition_key": PARTITION,
            "digest": format!("sha256:{}", "b".repeat(64))
        }]
    })
}

fn config() -> serde_json::Value {
    json!({
        "generation_profile": PROFILE,
        "prompt_checksum": format!("sha256:{}", "c".repeat(64)),
        "pipeline_revision": PIPELINE,
        "model_provider": "test-provider",
        "model_id": "test-model",
        "tokenizer_id": "test-tokenizer-v1",
        "policy_checksum": format!("sha256:{}", "d".repeat(64))
    })
}

fn memory_request(
    tenant: &str,
    source: &str,
    id: Option<String>,
    text: &str,
) -> UpsertSubjectMemoryRequest {
    UpsertSubjectMemoryRequest {
        id,
        tenant_id: tenant.to_string(),
        source_id: source.to_string(),
        memory_type: "project_fact".to_string(),
        text: text.to_string(),
        level: Some(0),
        source_digest: None,
        confidence: Some(0.9),
        last_seen_at: None,
        metadata: None,
        rollup_status: None,
        rollup_memory_key: None,
        rolled_up_at: None,
        status: None,
        created_at: None,
        updated_at: None,
    }
}

fn output<'a>(
    tenant: &str,
    source: &str,
    subject: &'a str,
    memory_key: &'a str,
    id: Option<String>,
    text: &str,
) -> SubjectMemoryGenerationOutput<'a> {
    SubjectMemoryGenerationOutput {
        subject_id: subject,
        memory_key,
        request: memory_request(tenant, source, id, text),
        source_digest: Some(format!("sha256:{}", "e".repeat(64))),
        rollup_status: "pending",
        generation_profile: PROFILE,
        project_partition_key: Some(PARTITION),
        payload_schema: Some("subject_memory_v2"),
        structured_payload: None,
        pipeline_revision: PIPELINE,
    }
}

async fn claimed_generation(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    suffix: &str,
) -> GenerationCommit {
    let job_id = format!("subject-memory-job-{suffix}");
    sqlx::query(
        "INSERT INTO engine_job_runs(id,job_type,trigger_type,tenant_id,source_id,status,started_at,data) \
         VALUES($1,'subject_memory','database_worker',$2,$3,'running',now(),'{}'::jsonb)",
    )
    .bind(&job_id)
    .bind(tenant)
    .bind(source)
    .execute(pool)
    .await
    .expect("create subject memory generation job");
    let manifest = manifest(format!("summary-{suffix}").as_str());
    let config = config();
    let prepared = crate::repositories::generation_commits::prepare_generation(
        pool,
        PrepareGeneration {
            tenant_id: tenant,
            source_id: source,
            job_run_id: &job_id,
            generation_step_key: "project-memory:project:alpha:0",
            generation_kind: "project_memory",
            input_digest: format!("sha256:{}", "a".repeat(64)).as_str(),
            input_manifest: &manifest,
            config_snapshot: &config,
            project_partition_key: Some(PARTITION),
        },
    )
    .await
    .expect("prepare subject memory generation");
    crate::repositories::generation_commits::claim_generation(
        pool,
        tenant,
        source,
        &prepared.generation_id,
        "subject-memory-worker",
        300,
    )
    .await
    .expect("claim subject memory generation")
    .expect("subject memory generation claim")
}

#[tokio::test]
async fn subject_memory_generation_is_immutable_idempotent_and_retires_previous_version() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = format!("project:{suffix}");
    let memory_key = "project:decision";

    let legacy = crate::repositories::subject_memories::upsert_subject_memory(
        &pool,
        &subject,
        memory_key,
        memory_request(&tenant, &source, None, "legacy"),
    )
    .await
    .expect("insert legacy memory");
    let legacy_replay = crate::repositories::subject_memories::upsert_subject_memory(
        &pool,
        &subject,
        memory_key,
        memory_request(&tenant, &source, None, "legacy updated"),
    )
    .await
    .expect("update legacy memory");
    assert_eq!(legacy_replay.id, legacy.id);

    let first_generation =
        claimed_generation(&pool, &tenant, &source, &format!("one-{suffix}")).await;
    let first = crate::repositories::generation_output_commits::commit_subject_memory_generation(
        &pool,
        &first_generation,
        output(
            &tenant,
            &source,
            &subject,
            memory_key,
            None,
            "generation one",
        ),
    )
    .await
    .expect("commit first generated memory");
    let replay = crate::repositories::generation_output_commits::commit_subject_memory_generation(
        &pool,
        &first_generation,
        output(
            &tenant,
            &source,
            &subject,
            memory_key,
            None,
            "ignored replay body",
        ),
    )
    .await
    .expect("replay committed callback");
    assert_eq!(replay.id, first.id);
    assert_eq!(replay.text, "generation one");

    let second_generation =
        claimed_generation(&pool, &tenant, &source, &format!("two-{suffix}")).await;
    let second = crate::repositories::generation_output_commits::commit_subject_memory_generation(
        &pool,
        &second_generation,
        output(
            &tenant,
            &source,
            &subject,
            memory_key,
            None,
            "generation two",
        ),
    )
    .await
    .expect("commit second generated memory");
    assert_ne!(second.id, first.id);

    let rows = sqlx::query_as::<_, (String, Option<String>, String, bool, Option<String>)>(
        "SELECT id,generation_id,status,recall_eligible,retired_reason \
         FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND memory_key=$4 ORDER BY created_at,id",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&subject)
    .bind(memory_key)
    .fetch_all(&pool)
    .await
    .expect("load subject memory versions");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .filter(|(_, _, status, eligible, _)| status == "active" && *eligible)
            .count(),
        1
    );
    let first_row = rows
        .iter()
        .find(|row| row.0 == first.id)
        .expect("first row");
    assert_eq!(first_row.2, "retired");
    assert!(!first_row.3);
    assert_eq!(first_row.4.as_deref(), Some("superseded_by_generation"));
    let second_row = rows
        .iter()
        .find(|row| row.0 == second.id)
        .expect("second row");
    assert_eq!(second_row.2, "active");
    assert!(second_row.3);
    let downstream = sqlx::query_as::<_, (String, sqlx::types::Json<serde_json::Value>)>(
        "SELECT event_kind,payload FROM engine_derived_event_outbox \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&second_generation.generation_id)
    .fetch_one(&pool)
    .await
    .expect("load subject memory rollup event");
    assert_eq!(downstream.0, "subject_memory_rollup_requested");
    assert!(downstream.1.get("text").is_none());
    let claimed = crate::repositories::derived_events::claim_publish_batch_for_scope(
        &pool,
        "subject-output-publisher",
        300,
        10,
        Some(&tenant),
        Some(&source),
    )
    .await
    .expect("claim subject memory downstream events");
    assert_eq!(claimed.len(), 2);
    for event in claimed {
        assert!(
            crate::repositories::derived_events::mark_published(&pool, &event)
                .await
                .expect("publish subject memory downstream event")
        );
    }

    let legacy_after_v2 = crate::repositories::subject_memories::upsert_subject_memory(
        &pool,
        &subject,
        memory_key,
        memory_request(&tenant, &source, None, "legacy management update"),
    )
    .await
    .expect("legacy management update remains available");
    assert_eq!(legacy_after_v2.id, legacy.id);
    let generated_text = sqlx::query_scalar::<_, String>(
        "SELECT data->>'text' FROM engine_subject_memories \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND generation_id=$4",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&second.id)
    .bind(&second_generation.generation_id)
    .fetch_one(&pool)
    .await
    .expect("load generated memory after legacy update");
    assert_eq!(generated_text, "generation two");
}

#[tokio::test]
async fn subject_memory_generation_rejects_stale_lease_and_rolls_back_partial_replacement() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = format!("project:{suffix}");
    let memory_key = "project:constraint";

    let baseline_generation =
        claimed_generation(&pool, &tenant, &source, &format!("baseline-{suffix}")).await;
    let baseline =
        crate::repositories::generation_output_commits::commit_subject_memory_generation(
            &pool,
            &baseline_generation,
            output(&tenant, &source, &subject, memory_key, None, "baseline"),
        )
        .await
        .expect("commit baseline memory");

    let stale = claimed_generation(&pool, &tenant, &source, &format!("stale-{suffix}")).await;
    sqlx::query(
        "UPDATE engine_generation_commits SET lease_expires_at=now()-interval '1 second' \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&stale.generation_id)
    .execute(&pool)
    .await
    .expect("expire stale subject memory lease");
    let takeover = crate::repositories::generation_commits::claim_generation(
        &pool,
        &tenant,
        &source,
        &stale.generation_id,
        "takeover-worker",
        300,
    )
    .await
    .expect("take over subject memory generation")
    .expect("takeover claim");
    let stale_error =
        crate::repositories::generation_output_commits::commit_subject_memory_generation(
            &pool,
            &stale,
            output(
                &tenant,
                &source,
                &subject,
                memory_key,
                None,
                "stale callback",
            ),
        )
        .await
        .expect_err("stale callback cannot replace active memory");
    assert!(stale_error.contains("lease is no longer current"));

    let duplicate_error =
        crate::repositories::generation_output_commits::commit_subject_memory_generation(
            &pool,
            &takeover,
            output(
                &tenant,
                &source,
                &subject,
                memory_key,
                Some(baseline.id.clone()),
                "duplicate id",
            ),
        )
        .await
        .expect_err("duplicate output id fails after staged retirement");
    assert!(duplicate_error.contains("duplicate key"));

    let baseline_state = sqlx::query_as::<_, (String, bool)>(
        "SELECT status,recall_eligible FROM engine_subject_memories \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&baseline.id)
    .fetch_one(&pool)
    .await
    .expect("load baseline after failed replacement");
    assert_eq!(baseline_state, ("active".to_string(), true));
    let failed_output_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM engine_subject_memories \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&takeover.generation_id)
    .fetch_one(&pool)
    .await
    .expect("count rolled back output");
    assert_eq!(failed_output_count, 0);
    let failed_event_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM engine_derived_event_outbox \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&takeover.generation_id)
    .fetch_one(&pool)
    .await
    .expect("count rolled back subject memory event");
    assert_eq!(failed_event_count, 0);

    let replacement =
        crate::repositories::generation_output_commits::commit_subject_memory_generation(
            &pool,
            &takeover,
            output(&tenant, &source, &subject, memory_key, None, "replacement"),
        )
        .await
        .expect("retry current lease after transactional rollback");
    assert_ne!(replacement.id, baseline.id);
}
