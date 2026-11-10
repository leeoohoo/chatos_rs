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
        memory_type: "project_summary".to_string(),
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
    generation: &'a GenerationCommit,
    tenant: &str,
    source: &str,
    subject: &'a str,
    memory_key: &'a str,
    id: Option<String>,
    text: &str,
) -> SubjectMemoryGenerationOutput<'a> {
    let source_refs = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .expect("generation inputs")
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .expect("input id")
                .to_string()
        })
        .collect::<Vec<_>>();
    let payload = Box::leak(Box::new(
        crate::models::structured_payload::build_generated_payload(
            generation.generation_id.as_str(),
            crate::models::structured_payload::GeneratedPayloadScope {
                kind: "project",
                subject_id: subject,
                project_partition_key: Some(PARTITION),
            },
            text,
            &source_refs,
            None,
        )
        .expect("test subject memory payload"),
    ));
    SubjectMemoryGenerationOutput {
        subject_id: subject,
        memory_key,
        request: memory_request(tenant, source, id, text),
        source_digest: Some(format!("sha256:{}", "e".repeat(64))),
        rollup_status: "pending",
        generation_profile: PROFILE,
        project_partition_key: Some(PARTITION),
        payload_schema: crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
        structured_payload: payload,
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

async fn claimed_final_generation(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    subject: &str,
    generation_kind: &str,
    profile: &str,
    input_kind: &str,
    input_ids: &[String],
    partition: Option<&str>,
    suffix: &str,
) -> GenerationCommit {
    let job = crate::repositories::control_plane::create_job_run(
        pool,
        crate::models::CreateEngineJobRunRequest {
            job_type: "subject_memory".to_string(),
            trigger_type: "contract_test".to_string(),
            tenant_id: Some(tenant.to_string()),
            source_id: Some(source.to_string()),
            thread_id: None,
            subject_id: Some(subject.to_string()),
            thread_label: Some("contract-test".to_string()),
            metadata: None,
        },
    )
    .await
    .expect("create finalization job");
    let inputs = input_ids
        .iter()
        .enumerate()
        .map(|(ordinal, id)| {
            json!({
                "kind": input_kind,
                "id": id,
                "ordinal": ordinal,
                "project_partition_key": partition,
                "digest": format!("sha256:{}", "b".repeat(64))
            })
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "schema_version": "memory_generation_input_v1",
        "digest_algorithm": "sha256",
        "canonicalizer_revision": "canonical-json-v1",
        "inputs": inputs
    });
    let config = json!({
        "generation_profile": profile,
        "prompt_checksum": format!("sha256:{}", "c".repeat(64)),
        "pipeline_revision": PIPELINE,
        "model_provider": "test-provider",
        "model_id": "test-model",
        "tokenizer_id": "test-tokenizer-v1",
        "policy_checksum": format!("sha256:{}", "d".repeat(64))
    });
    let prepared = crate::repositories::generation_commits::prepare_generation(
        pool,
        PrepareGeneration {
            tenant_id: tenant,
            source_id: source,
            job_run_id: &job.id,
            generation_step_key: &format!("{generation_kind}:{suffix}"),
            generation_kind,
            input_digest: format!("sha256:{}", "a".repeat(64)).as_str(),
            input_manifest: &manifest,
            config_snapshot: &config,
            project_partition_key: partition,
        },
    )
    .await
    .expect("prepare finalization generation");
    crate::repositories::generation_commits::claim_generation(
        pool,
        tenant,
        source,
        &prepared.generation_id,
        "subject-finalization-worker",
        300,
    )
    .await
    .expect("claim finalization generation")
    .expect("finalization generation claim")
}

fn successful_job_result(input_count: usize) -> crate::models::FinishEngineJobRunRequest {
    crate::models::FinishEngineJobRunRequest {
        status: "done".to_string(),
        input_count: input_count as i64,
        output_count: 1,
        processed_count: input_count as i64,
        success_count: input_count as i64,
        error_count: 0,
        metadata: Some(json!({"atomic_finalization": true})),
        error_message: None,
    }
}

async fn source_summary(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    subject: &str,
    suffix: &str,
) -> crate::models::EngineSummary {
    let thread_id = format!("thread-{suffix}");
    crate::repositories::threads::upsert_thread(
        pool,
        &thread_id,
        crate::models::UpsertThreadRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
            subject_id: format!("agent:{suffix}"),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec![subject.to_string()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create subject memory source thread");
    let summary = crate::repositories::summaries::insert_test_thread_summary(
        pool,
        tenant,
        source,
        &thread_id,
        format!("agent:{suffix}").as_str(),
        "source summary",
        None,
        None,
        1,
        Some(PARTITION),
    )
    .await
    .expect("create subject memory source summary");
    sqlx::query(
        "UPDATE engine_summaries SET project_partition_key=$4 \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(tenant)
    .bind(source)
    .bind(&summary.id)
    .bind(PARTITION)
    .execute(pool)
    .await
    .expect("set source summary partition");
    summary
}

fn final_output<'a>(
    generation: &'a GenerationCommit,
    tenant: &str,
    source: &str,
    subject: &'a str,
    memory_key: &'a str,
    memory_type: &str,
    level: i64,
    profile: &'a str,
) -> SubjectMemoryGenerationOutput<'a> {
    let mut request = memory_request(tenant, source, None, "atomic generated memory");
    request.memory_type = memory_type.to_string();
    request.level = Some(level);
    request.rollup_status = Some("pending".to_string());
    let source_refs = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .expect("generation inputs")
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .expect("input id")
                .to_string()
        })
        .collect::<Vec<_>>();
    let payload = Box::leak(Box::new(
        crate::models::structured_payload::build_generated_payload(
            generation.generation_id.as_str(),
            crate::models::structured_payload::GeneratedPayloadScope {
                kind: "project",
                subject_id: subject,
                project_partition_key: Some(PARTITION),
            },
            request.text.as_str(),
            &source_refs,
            None,
        )
        .expect("test final subject memory payload"),
    ));
    SubjectMemoryGenerationOutput {
        subject_id: subject,
        memory_key,
        request,
        source_digest: Some(format!("sha256:{}", "e".repeat(64))),
        rollup_status: "pending",
        generation_profile: profile,
        project_partition_key: Some(PARTITION),
        payload_schema: crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
        structured_payload: payload,
        pipeline_revision: PIPELINE,
    }
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

    let legacy = crate::repositories::subject_memories::insert_test_subject_memory(
        &pool,
        &subject,
        memory_key,
        memory_request(&tenant, &source, None, "legacy"),
    )
    .await
    .expect("insert legacy memory");
    let first_generation =
        claimed_generation(&pool, &tenant, &source, &format!("one-{suffix}")).await;
    let first = crate::repositories::generation_output_commits::commit_subject_memory_generation(
        &pool,
        &first_generation,
        output(
            &first_generation,
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
            &first_generation,
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
            &second_generation,
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

    let legacy_state = sqlx::query_as::<_, (String, bool, Option<String>)>(
        "SELECT status,recall_eligible,retired_reason FROM engine_subject_memories \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&legacy.id)
    .fetch_one(&pool)
    .await
    .expect("load archived legacy memory");
    assert_eq!(legacy_state.0, "retired");
    assert!(!legacy_state.1);
    assert_eq!(legacy_state.2.as_deref(), Some("superseded_by_generation"));
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
            output(
                &baseline_generation,
                &tenant,
                &source,
                &subject,
                memory_key,
                None,
                "baseline",
            ),
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
                &stale,
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
                &takeover,
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
            output(
                &takeover,
                &tenant,
                &source,
                &subject,
                memory_key,
                None,
                "replacement",
            ),
        )
        .await
        .expect("retry current lease after transactional rollback");
    assert_ne!(replacement.id, baseline.id);
}

#[tokio::test]
async fn subject_memory_l0_final_commit_is_atomic_and_idempotent() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = PARTITION.to_string();
    let scope_key = format!("identity:{subject}");
    let summary = source_summary(&pool, &tenant, &source, &subject, &suffix).await;
    let source_ids = vec![summary.id.clone()];
    let generation = claimed_final_generation(
        &pool,
        &tenant,
        &source,
        &subject,
        "project_memory",
        PROFILE,
        "summary",
        &source_ids,
        Some(PARTITION),
        &suffix,
    )
    .await;
    crate::repositories::subject_memory_scopes::upsert_subject_memory_scope(
        &pool,
        &scope_key,
        crate::models::UpsertSubjectMemoryScopeRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            memory_type: "project_summary".to_string(),
            source_thread_label: subject.clone(),
            relation_subject_id: Some(subject.clone()),
            source_summary_type: Some("thread_incremental".to_string()),
            prompt_title: None,
            memory_metadata: None,
            status: Some("active".to_string()),
        },
    )
    .await
    .expect("create subject scope");
    assert!(
        crate::repositories::subject_memory_scopes::try_acquire_subject_memory_scope_slot(
            &pool,
            &tenant,
            &source,
            &scope_key,
            &generation.job_run_id,
            300,
        )
        .await
        .expect("claim subject scope")
    );

    let memory =
        crate::repositories::generation_output_commits::commit_subject_memory_generation_final(
            &pool,
            &generation,
            final_output(
                &generation,
                &tenant,
                &source,
                &subject,
                "project:l0:atomic",
                "project_summary",
                0,
                PROFILE,
            ),
            crate::repositories::generation_output_commits::SubjectMemoryGenerationFinalization {
                sources: crate::repositories::generation_output_commits::SubjectMemoryGenerationSources::Summaries {
                    summary_ids: &source_ids,
                    scope_key: Some(&scope_key),
                },
                scope_claim: Some(crate::repositories::generation_output_commits::SubjectMemoryScopeClaim {
                    scope_key: &scope_key,
                    lock_owner: &generation.job_run_id,
                }),
                job_result: successful_job_result(1),
            },
        )
        .await
        .expect("atomically finalize subject memory L0");

    let replay =
        crate::repositories::generation_output_commits::commit_subject_memory_generation_final(
            &pool,
            &generation,
            final_output(
                &generation,
                &tenant,
                &source,
                &subject,
                "project:l0:atomic",
                "project_summary",
                0,
                PROFILE,
            ),
            crate::repositories::generation_output_commits::SubjectMemoryGenerationFinalization {
                sources: crate::repositories::generation_output_commits::SubjectMemoryGenerationSources::Summaries {
                    summary_ids: &source_ids,
                    scope_key: Some(&scope_key),
                },
                scope_claim: Some(crate::repositories::generation_output_commits::SubjectMemoryScopeClaim {
                    scope_key: &scope_key,
                    lock_owner: &generation.job_run_id,
                }),
                job_result: successful_job_result(1),
            },
        )
        .await
        .expect("replay subject memory L0 callback");
    assert_eq!(replay.id, memory.id);

    let state = sqlx::query_as::<_, (i64, Vec<String>, String, String, i64, i64)>(
        "SELECT s.subject_memory_summarized,s.subject_memory_scope_keys,j.status,sc.subject_memory_status, \
         (SELECT COUNT(*) FROM engine_subject_memories m WHERE m.tenant_id=$1 AND m.source_id=$2 AND m.generation_id=$5), \
         (SELECT COUNT(*) FROM engine_derived_event_outbox e WHERE e.tenant_id=$1 AND e.source_id=$2 AND e.generation_id=$5) \
         FROM engine_summaries s JOIN engine_job_runs j ON j.id=$3 \
         JOIN engine_subject_memory_scopes sc ON sc.tenant_id=$1 AND sc.source_id=$2 AND sc.scope_key=$4 \
         WHERE s.tenant_id=$1 AND s.source_id=$2 AND s.id=$6",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&generation.job_run_id)
    .bind(&scope_key)
    .bind(&generation.generation_id)
    .bind(&summary.id)
    .fetch_one(&pool)
    .await
    .expect("load atomic subject L0 state");
    assert_eq!(state.0, 1);
    assert_eq!(state.1, vec![scope_key]);
    assert_eq!(state.2, "done");
    assert_eq!(state.3, "idle");
    assert_eq!(state.4, 1);
    assert_eq!(state.5, 1);
}

#[tokio::test]
async fn subject_memory_scope_reenters_after_release_and_fences_an_active_owner() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = format!("project:{suffix}");
    let scope_key = format!("identity:{subject}");
    crate::repositories::subject_memory_scopes::upsert_subject_memory_scope(
        &pool,
        &scope_key,
        crate::models::UpsertSubjectMemoryScopeRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            memory_type: "project_summary".to_string(),
            source_thread_label: subject.clone(),
            relation_subject_id: Some(subject),
            source_summary_type: Some("thread_incremental".to_string()),
            prompt_title: None,
            memory_metadata: None,
            status: Some("active".to_string()),
        },
    )
    .await
    .expect("create restart-safe subject scope");
    let parent = crate::repositories::control_plane::create_job_run(
        &pool,
        crate::models::CreateEngineJobRunRequest {
            job_type: "subject_memory".to_string(),
            trigger_type: "restart-contract".to_string(),
            tenant_id: Some(tenant.clone()),
            source_id: Some(source.clone()),
            thread_id: None,
            subject_id: None,
            thread_label: None,
            metadata: None,
        },
    )
    .await
    .expect("create durable subject parent");
    assert!(
        crate::repositories::subject_memory_scopes::try_acquire_subject_memory_scope_slot(
            &pool, &tenant, &source, &scope_key, &parent.id, 300,
        )
        .await
        .expect("claim original subject scope")
    );
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET lock_expires_at=now()-interval '1 second' \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&scope_key)
    .execute(&pool)
    .await
    .expect("expire original subject scope");
    assert!(
        !crate::repositories::subject_memory_scopes::try_acquire_subject_memory_scope_slot(
            &pool,
            &tenant,
            &source,
            &scope_key,
            "subject-other",
            300,
        )
        .await
        .expect("reject subject takeover from a running parent")
    );
    crate::repositories::subject_memory_scopes::release_subject_memory_scope_slot(
        &pool, &tenant, &source, &scope_key, &parent.id,
    )
    .await
    .expect("release original subject scope");
    assert!(
        crate::repositories::subject_memory_scopes::refresh_subject_memory_scope_slot(
            &pool, &tenant, &source, &scope_key, &parent.id, 300,
        )
        .await
        .expect("reenter released subject scope")
    );
    assert!(
        !crate::repositories::subject_memory_scopes::refresh_subject_memory_scope_slot(
            &pool,
            &tenant,
            &source,
            &scope_key,
            "subject-other",
            300,
        )
        .await
        .expect("fence different subject scope owner")
    );
}

#[tokio::test]
async fn subject_memory_rollup_final_commit_updates_sources_and_job_atomically() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = format!("project:{suffix}");
    let source_memory = crate::repositories::subject_memories::insert_test_subject_memory(
        &pool,
        &subject,
        "project:l0:source",
        {
            let mut request = memory_request(&tenant, &source, None, "source memory");
            request.memory_type = "project_summary".to_string();
            request.rollup_status = Some("pending".to_string());
            request
        },
    )
    .await
    .expect("create rollup source memory");
    sqlx::query(
        "UPDATE engine_subject_memories SET project_partition_key=$4 \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&source_memory.id)
    .bind(PARTITION)
    .execute(&pool)
    .await
    .expect("set source memory partition");
    let source_ids = vec![source_memory.id.clone()];
    let profile = "subject_memory_rollup_v2";
    let generation = claimed_final_generation(
        &pool,
        &tenant,
        &source,
        &subject,
        "subject_memory_rollup",
        profile,
        "subject_memory",
        &source_ids,
        Some(PARTITION),
        &suffix,
    )
    .await;
    let output_key = "project:l1:atomic";
    let output =
        crate::repositories::generation_output_commits::commit_subject_memory_generation_final(
            &pool,
            &generation,
            final_output(
                &generation,
                &tenant,
                &source,
                &subject,
                output_key,
                "project_summary",
                1,
                profile,
            ),
            crate::repositories::generation_output_commits::SubjectMemoryGenerationFinalization {
                sources: crate::repositories::generation_output_commits::SubjectMemoryGenerationSources::SubjectMemories {
                    memory_ids: &source_ids,
                },
                scope_claim: None,
                job_result: successful_job_result(1),
            },
        )
        .await
        .expect("atomically finalize subject memory rollup");
    let state = sqlx::query_as::<_, (String, Option<String>, String, String)>(
        "SELECT source.rollup_status,source.data->>'rollup_memory_key',job.status,registry.status \
         FROM engine_subject_memories source JOIN engine_job_runs job ON job.id=$4 \
         JOIN engine_generation_commits registry ON registry.tenant_id=$1 AND registry.source_id=$2 \
          AND registry.generation_id=$5 WHERE source.tenant_id=$1 AND source.source_id=$2 AND source.id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&source_memory.id)
    .bind(&generation.job_run_id)
    .bind(&generation.generation_id)
    .fetch_one(&pool)
    .await
    .expect("load atomic rollup state");
    assert_eq!(state.0, "done");
    assert_eq!(state.1.as_deref(), Some(output_key));
    assert_eq!(state.2, "done");
    assert_eq!(state.3, "committed");
    assert_eq!(output.level, 1);
}
