// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use sqlx::types::Json;
use sqlx::{Postgres, Transaction};

use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineJobRun, EngineRecord, EngineSubjectMemory, EngineSummary,
    FinishEngineJobRunRequest, UpsertSubjectMemoryRequest,
};
use crate::repositories::derived_events;
use crate::repositories::generation_commits::{self, GenerationCommit};
use crate::repositories::lineage;
use crate::repositories::memory_facts::{self, FactOutputScope};
use crate::repositories::postgres::{decode, json, timestamp};
use crate::repositories::subject_memories::common::{
    build_subject_memory, lock_subject_memory_logical_key,
};
use crate::repositories::summaries::common::new_summary;

pub struct SubjectMemoryGenerationOutput<'a> {
    pub subject_id: &'a str,
    pub memory_key: &'a str,
    pub request: UpsertSubjectMemoryRequest,
    pub source_digest: Option<String>,
    pub rollup_status: &'a str,
    pub generation_profile: &'a str,
    pub project_partition_key: Option<&'a str>,
    pub payload_schema: &'a str,
    pub structured_payload: &'a serde_json::Value,
    pub pipeline_revision: &'a str,
}

pub struct ThreadSummaryGenerationOutput<'a> {
    pub thread_id: &'a str,
    pub subject_id: &'a str,
    pub summary_type: &'a str,
    pub level: i64,
    pub source_digest: Option<String>,
    pub summary_text: &'a str,
    pub source_record_start_id: Option<String>,
    pub source_record_end_id: Option<String>,
    pub source_record_count: usize,
    pub metadata: Option<serde_json::Value>,
    pub generation_reason: &'a str,
    pub generation_profile: &'a str,
    pub project_partition_key: &'a str,
    pub payload_schema: &'a str,
    pub structured_payload: &'a serde_json::Value,
    pub pipeline_revision: &'a str,
}

pub struct ThreadL0GenerationFinalization<'a> {
    pub record_ids: &'a [String],
    pub consumed_summary_tokens: i64,
    pub job_result: FinishEngineJobRunRequest,
}

pub struct ThreadL0ResummarizationFinalization<'a> {
    pub previous_summary_id: &'a str,
    pub record_ids: &'a [String],
    pub job_result: FinishEngineJobRunRequest,
}

pub struct ThreadRollupGenerationFinalization<'a> {
    pub summary_ids: &'a [String],
    pub job_result: FinishEngineJobRunRequest,
}

pub enum SubjectMemoryGenerationSources<'a> {
    Summaries {
        summary_ids: &'a [String],
        scope_key: Option<&'a str>,
    },
    SubjectMemories {
        memory_ids: &'a [String],
    },
}

pub struct SubjectMemoryScopeClaim<'a> {
    pub scope_key: &'a str,
    pub lock_owner: &'a str,
}

pub struct SubjectMemoryGenerationFinalization<'a> {
    pub sources: SubjectMemoryGenerationSources<'a>,
    pub scope_claim: Option<SubjectMemoryScopeClaim<'a>>,
    pub job_result: FinishEngineJobRunRequest,
}

pub async fn commit_thread_summary_generation(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, None, None).await
}

pub async fn commit_thread_summary_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadL0GenerationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, Some(finalization), None, None)
        .await
}

pub async fn commit_thread_summary_resummarization_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadL0ResummarizationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, None, Some(finalization))
        .await
}

pub async fn commit_thread_rollup_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadRollupGenerationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, Some(finalization), None)
        .await
}

async fn commit_thread_summary_generation_inner(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    l0_finalization: Option<ThreadL0GenerationFinalization<'_>>,
    rollup_finalization: Option<ThreadRollupGenerationFinalization<'_>>,
    resummarization_finalization: Option<ThreadL0ResummarizationFinalization<'_>>,
) -> Result<EngineSummary, String> {
    let current = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "thread summary generation registry row is missing".to_string())?;
    validate_summary_output(&current, &output)?;
    if current.status == "committed" {
        return load_committed_thread_summary(db, &current).await;
    }
    if current.status != "running" || current.lease_token != generation.lease_token {
        return Err("thread summary generation commit lease is no longer current".to_string());
    }
    if let Some(finalization) = l0_finalization.as_ref() {
        validate_l0_finalization(&current, &output, finalization)?;
    }
    if let Some(finalization) = rollup_finalization.as_ref() {
        validate_rollup_finalization(&current, &output, finalization)?;
    }
    if let Some(finalization) = resummarization_finalization.as_ref() {
        validate_resummarization_finalization(&current, &output, finalization)?;
    }

    let project_partition_key = output.project_partition_key;
    let is_noop = output.summary_type == "thread_noop";
    let mut summary = new_summary(
        &current.tenant_id,
        &current.source_id,
        output.thread_id,
        output.subject_id,
        output.summary_type,
        output.level,
        output.source_digest,
        output.summary_text,
        output.source_record_start_id,
        output.source_record_end_id,
        output.source_record_count,
        output.metadata,
    );
    summary.payload_schema = Some(output.payload_schema.to_string());
    summary.structured_payload = Some(output.structured_payload.clone());
    if is_noop {
        summary.rollup_status = "done".to_string();
        summary.subject_memory_summarized = 1;
        summary.subject_memory_summarized_at = Some(summary.created_at.clone());
    }
    let downstream_pending = !is_noop;
    let downstream_version = i64::from(downstream_pending);
    let downstream_requested_at = downstream_pending
        .then(|| timestamp(&summary.created_at))
        .transpose()?;
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    if l0_finalization.is_some()
        || rollup_finalization.is_some()
        || resummarization_finalization.is_some()
    {
        lock_running_generation_job(&mut tx, &current, Some(output.thread_id), None).await?;
    }
    if let Some(finalization) = resummarization_finalization.as_ref() {
        supersede_resummarized_range(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    sqlx::query(
        "INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type, \
         level,source_digest,status,rollup_status,subject_memory_summarized,rollup_dispatch_pending, \
         rollup_dispatch_version,rollup_dispatch_requested_at,subject_memory_source_dispatch_pending, \
         subject_memory_source_dispatch_version,subject_memory_source_dispatch_requested_at, \
         created_at,updated_at,data,generation_id,generation_reason,generation_profile, \
         project_partition_key,recall_eligible,payload_schema,structured_payload,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$12,$13,$14,$15,$16,$17, \
         $18,$19,$20,$21,true,$22,$23,$24)",
    )
    .bind(&summary.id)
    .bind(&summary.tenant_id)
    .bind(&summary.source_id)
    .bind(&summary.thread_id)
    .bind(&summary.subject_id)
    .bind(&summary.summary_type)
    .bind(summary.level)
    .bind(&summary.source_digest)
    .bind(&summary.status)
    .bind(&summary.rollup_status)
    .bind(summary.subject_memory_summarized)
    .bind(downstream_pending)
    .bind(downstream_version)
    .bind(downstream_requested_at)
    .bind(timestamp(&summary.created_at)?)
    .bind(timestamp(&summary.updated_at)?)
    .bind(json(&summary)?)
    .bind(&current.generation_id)
    .bind(output.generation_reason)
    .bind(output.generation_profile)
    .bind(output.project_partition_key)
    .bind(output.payload_schema)
    .bind(Json(output.structured_payload))
    .bind(output.pipeline_revision)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if !is_noop {
        memory_facts::project_payload(
            &mut tx,
            &current,
            FactOutputScope {
                output_kind: "thread_summary",
                output_id: summary.id.as_str(),
                scope_kind: "thread",
                scope_key: summary.thread_id.as_str(),
                subject_id: summary.subject_id.as_str(),
                project_partition_key: Some(project_partition_key),
            },
            output.payload_schema,
            output.structured_payload,
        )
        .await?;
    }
    if !is_noop {
        derived_events::enqueue_summary_downstream_requests(
            &mut tx,
            &current.tenant_id,
            &current.source_id,
            &summary.id,
            &summary.thread_id,
            project_partition_key,
            &current.generation_id,
        )
        .await?;
    }
    if let Some(finalization) = l0_finalization {
        finalize_thread_l0_generation(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    if let Some(finalization) = rollup_finalization {
        finalize_thread_rollup_generation(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    if let Some(finalization) = resummarization_finalization {
        finalize_thread_l0_resummarization(
            &mut tx,
            &current,
            &summary,
            project_partition_key,
            finalization,
        )
        .await?;
    }
    if !generation_commits::commit_generation(
        &mut tx,
        generation,
        "thread_summary",
        summary.id.as_str(),
    )
    .await?
    {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return load_committed_thread_summary(db, &current).await;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(summary)
}

pub async fn commit_subject_memory_generation(
    db: &Db,
    generation: &GenerationCommit,
    output: SubjectMemoryGenerationOutput<'_>,
) -> Result<EngineSubjectMemory, String> {
    commit_subject_memory_generation_inner(db, generation, output, None).await
}

pub async fn commit_subject_memory_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: SubjectMemoryGenerationOutput<'_>,
    finalization: SubjectMemoryGenerationFinalization<'_>,
) -> Result<EngineSubjectMemory, String> {
    commit_subject_memory_generation_inner(db, generation, output, Some(finalization)).await
}

async fn commit_subject_memory_generation_inner(
    db: &Db,
    generation: &GenerationCommit,
    output: SubjectMemoryGenerationOutput<'_>,
    finalization: Option<SubjectMemoryGenerationFinalization<'_>>,
) -> Result<EngineSubjectMemory, String> {
    let current = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "subject memory generation registry row is missing".to_string())?;
    validate_subject_output(&current, &output)?;
    if current.status == "committed" {
        return load_committed_subject_memory(db, &current).await;
    }
    if current.status != "running" || current.lease_token != generation.lease_token {
        return Err("subject memory generation commit lease is no longer current".to_string());
    }
    if let Some(finalization) = finalization.as_ref() {
        validate_subject_finalization(&current, &output, finalization)?;
    }
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    if finalization.is_some() {
        lock_running_generation_job(&mut tx, &current, None, Some(output.subject_id)).await?;
    }
    lock_subject_memory_logical_key(
        &mut tx,
        &current.tenant_id,
        &current.source_id,
        output.subject_id,
        output.memory_key,
    )
    .await?;
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND memory_key=$4 AND status='active' AND recall_eligible FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(output.subject_id)
    .bind(output.memory_key)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let now = now_rfc3339();
    for row in rows {
        let mut previous = decode::<EngineSubjectMemory>(row)?;
        previous.status = "retired".to_string();
        previous.updated_at = now.clone();
        sqlx::query(
            "UPDATE engine_subject_memories SET status='retired',recall_eligible=false, \
             retired_reason='superseded_by_generation',updated_at=$5,data=$6 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='active' AND memory_key=$4",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(&previous.id)
        .bind(output.memory_key)
        .bind(timestamp(&now)?)
        .bind(json(&previous)?)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        memory_facts::retire_output(
            &mut tx,
            &generation.tenant_id,
            &generation.source_id,
            "subject_memory",
            previous.id.as_str(),
        )
        .await?;
    }

    let mut memory = build_subject_memory(
        None,
        output.subject_id,
        output.memory_key,
        output.request,
        Some(output.source_digest),
        Some(output.rollup_status),
    );
    memory.payload_schema = Some(output.payload_schema.to_string());
    memory.structured_payload = Some(output.structured_payload.clone());
    let relation_subject_id = memory
        .metadata
        .as_ref()
        .and_then(|value| value.get("relation_subject_id"))
        .and_then(serde_json::Value::as_str);
    sqlx::query(
        "INSERT INTO engine_subject_memories(id,tenant_id,source_id,subject_id,memory_key,memory_type, \
         level,source_digest,relation_subject_id,status,rollup_status,created_at,updated_at,data, \
         generation_id,generation_profile,project_partition_key,recall_eligible,payload_schema, \
         structured_payload,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,true,$18,$19,$20)",
    )
    .bind(&memory.id)
    .bind(&memory.tenant_id)
    .bind(&memory.source_id)
    .bind(&memory.subject_id)
    .bind(&memory.memory_key)
    .bind(&memory.memory_type)
    .bind(memory.level)
    .bind(&memory.source_digest)
    .bind(relation_subject_id)
    .bind(&memory.status)
    .bind(&memory.rollup_status)
    .bind(timestamp(&memory.created_at)?)
    .bind(timestamp(&memory.updated_at)?)
    .bind(json(&memory)?)
    .bind(&generation.generation_id)
    .bind(output.generation_profile)
    .bind(output.project_partition_key)
    .bind(output.payload_schema)
    .bind(Json(output.structured_payload))
    .bind(output.pipeline_revision)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let (scope_kind, scope_key) = match memory.memory_type.as_str() {
        "project_summary" => (
            "project",
            output.project_partition_key.ok_or_else(|| {
                "project subject memory output has no project partition".to_string()
            })?,
        ),
        "agent_memory" => ("agent", memory.subject_id.as_str()),
        _ => return Err("subject memory output has unsupported payload scope".to_string()),
    };
    memory_facts::project_payload(
        &mut tx,
        &current,
        FactOutputScope {
            output_kind: "subject_memory",
            output_id: memory.id.as_str(),
            scope_kind,
            scope_key,
            subject_id: memory.subject_id.as_str(),
            project_partition_key: output.project_partition_key,
        },
        output.payload_schema,
        output.structured_payload,
    )
    .await?;
    derived_events::enqueue_subject_memory_rollup_request(
        &mut tx,
        &generation.tenant_id,
        &generation.source_id,
        &memory.id,
        &memory.subject_id,
        output.project_partition_key,
        &generation.generation_id,
    )
    .await?;

    if let Some(finalization) = finalization {
        finalize_subject_memory_generation(&mut tx, &current, &memory, finalization).await?;
    }

    if !generation_commits::commit_generation(
        &mut tx,
        generation,
        "subject_memory",
        memory.id.as_str(),
    )
    .await?
    {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return load_committed_subject_memory(db, generation).await;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(memory)
}

fn validate_subject_finalization(
    generation: &GenerationCommit,
    output: &SubjectMemoryGenerationOutput<'_>,
    finalization: &SubjectMemoryGenerationFinalization<'_>,
) -> Result<(), String> {
    let (expected_kind, source_ids) = match &finalization.sources {
        SubjectMemoryGenerationSources::Summaries { summary_ids, .. } => {
            if output.request.level.unwrap_or(0) != 0 {
                return Err(
                    "summary sources can only finalize a level-0 subject memory".to_string()
                );
            }
            let expected = match output.request.memory_type.as_str() {
                "project_summary" => "project_memory",
                "agent_memory" => "agent_memory",
                _ => {
                    return Err(
                        "level-0 subject memory type has no generation kind mapping".to_string()
                    )
                }
            };
            (expected, *summary_ids)
        }
        SubjectMemoryGenerationSources::SubjectMemories { memory_ids } => {
            if output.request.level.unwrap_or(0) <= 0 {
                return Err("subject-memory rollup output level must be positive".to_string());
            }
            ("subject_memory_rollup", *memory_ids)
        }
    };
    if generation.generation_kind != expected_kind {
        return Err(
            "subject memory finalization source kind does not match generation".to_string(),
        );
    }
    validate_frozen_source_ids(generation, source_ids, "subject memory")?;
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err(
            "subject memory finalization requires a successful terminal job result".to_string(),
        );
    }
    match (&finalization.sources, &finalization.scope_claim) {
        (SubjectMemoryGenerationSources::Summaries { scope_key, .. }, Some(scope_claim))
            if scope_key.map(str::trim) == Some(scope_claim.scope_key.trim()) => {}
        (
            SubjectMemoryGenerationSources::Summaries {
                scope_key: None, ..
            },
            None,
        ) => {}
        (SubjectMemoryGenerationSources::SubjectMemories { .. }, None)
        | (SubjectMemoryGenerationSources::SubjectMemories { .. }, Some(_)) => {}
        _ => {
            return Err(
                "subject memory scope claim must exactly match the frozen summary scope"
                    .to_string(),
            )
        }
    }
    if finalization.scope_claim.as_ref().is_some_and(|claim| {
        claim.scope_key.trim().is_empty() || claim.lock_owner.trim().is_empty()
    }) {
        return Err("subject memory scope claim identity is incomplete".to_string());
    }
    Ok(())
}

fn validate_frozen_source_ids(
    generation: &GenerationCommit,
    source_ids: &[String],
    label: &str,
) -> Result<(), String> {
    if source_ids.is_empty() {
        return Err(format!("{label} finalization sources are empty"));
    }
    let unique = source_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != source_ids.len() || unique.iter().any(|value| value.trim().is_empty()) {
        return Err(format!(
            "{label} finalization source ids must be unique and non-empty"
        ));
    }
    let frozen_ids = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{label} generation input manifest has no inputs"))?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("{label} generation input manifest has an invalid id"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let requested_ids = source_ids.iter().map(String::as_str).collect::<Vec<_>>();
    if frozen_ids != requested_ids {
        return Err(format!(
            "{label} finalization sources do not match frozen input order"
        ));
    }
    Ok(())
}

async fn finalize_subject_memory_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSubjectMemory,
    finalization: SubjectMemoryGenerationFinalization<'_>,
) -> Result<(), String> {
    let now = now_rfc3339();
    match finalization.sources {
        SubjectMemoryGenerationSources::Summaries {
            summary_ids,
            scope_key,
        } => {
            finalize_subject_summary_sources(tx, generation, summary_ids, scope_key, &now).await?;
            lineage::insert_subject_summary_edges(tx, generation, output, summary_ids).await?;
        }
        SubjectMemoryGenerationSources::SubjectMemories { memory_ids } => {
            finalize_subject_memory_sources(tx, generation, output, memory_ids, &now).await?;
            lineage::insert_subject_memory_rollup_edges(tx, generation, output, memory_ids).await?;
        }
    }
    if let Some(claim) = finalization.scope_claim {
        let released = sqlx::query(
            "UPDATE engine_subject_memory_scopes SET subject_memory_status='idle',lock_owner=NULL, \
             lock_expires_at=NULL,updated_at=now() WHERE tenant_id=$1 AND source_id=$2 \
             AND scope_key=$3 AND status='active' AND ( \
               (subject_memory_status='running' AND lock_owner=$4) OR \
               (subject_memory_status<>'running' AND (lock_owner IS NULL OR lock_owner=$4)))",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(claim.scope_key)
        .bind(claim.lock_owner)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if released.rows_affected() != 1 {
            return Err("subject memory scope claim was lost before final commit".to_string());
        }
    }
    finish_subject_generation_job(
        tx,
        generation,
        &output.subject_id,
        finalization.job_result,
        &now,
    )
    .await
}

async fn finalize_subject_summary_sources(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    summary_ids: &[String],
    scope_key: Option<&str>,
    now: &str,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 \
         AND id=ANY($3) AND status='done' AND recall_eligible AND \
         (($4::text IS NULL AND subject_memory_summarized<>1) OR \
          ($4::text IS NOT NULL AND NOT ($4=ANY(subject_memory_scope_keys)))) FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(summary_ids)
    .bind(scope_key)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != summary_ids.len() {
        return Err("subject memory finalization did not own every frozen summary".to_string());
    }
    for row in rows {
        let mut source: EngineSummary = decode(row)?;
        source.subject_memory_summarized = 1;
        source.subject_memory_summarized_at = Some(now.to_string());
        source.updated_at = now.to_string();
        let updated = if let Some(scope_key) = scope_key {
            sqlx::query(
                "UPDATE engine_summaries SET subject_memory_summarized=1, \
                 subject_memory_scope_keys=array_append(subject_memory_scope_keys,$4), \
                 updated_at=$5,data=$6 WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
                 AND NOT ($4=ANY(subject_memory_scope_keys))",
            )
            .bind(&source.tenant_id)
            .bind(&source.source_id)
            .bind(&source.id)
            .bind(scope_key)
            .bind(timestamp(now)?)
            .bind(json(&source)?)
            .execute(&mut **tx)
            .await
        } else {
            sqlx::query(
                "UPDATE engine_summaries SET subject_memory_summarized=1,updated_at=$4,data=$5 \
                 WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
                 AND subject_memory_summarized<>1",
            )
            .bind(&source.tenant_id)
            .bind(&source.source_id)
            .bind(&source.id)
            .bind(timestamp(now)?)
            .bind(json(&source)?)
            .execute(&mut **tx)
            .await
        }
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("subject memory summary source changed during final commit".to_string());
        }
    }

    Ok(())
}

async fn finalize_subject_memory_sources(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSubjectMemory,
    memory_ids: &[String],
    now: &str,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND id=ANY($4) AND status='active' AND recall_eligible \
         AND rollup_status='pending' FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&output.subject_id)
    .bind(memory_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != memory_ids.len() {
        return Err("subject memory rollup did not own every frozen source memory".to_string());
    }
    for row in rows {
        let mut source: EngineSubjectMemory = decode(row)?;
        source.rollup_status = "done".to_string();
        source.rollup_memory_key = Some(output.memory_key.clone());
        source.rolled_up_at = Some(now.to_string());
        source.updated_at = now.to_string();
        let updated = sqlx::query(
            "UPDATE engine_subject_memories SET rollup_status='done',updated_at=$4,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='active' \
             AND recall_eligible AND rollup_status='pending'",
        )
        .bind(&source.tenant_id)
        .bind(&source.source_id)
        .bind(&source.id)
        .bind(timestamp(now)?)
        .bind(json(&source)?)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("subject memory rollup source changed during final commit".to_string());
        }
    }
    Ok(())
}

async fn finish_subject_generation_job(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    subject_id: &str,
    result: FinishEngineJobRunRequest,
    now: &str,
) -> Result<(), String> {
    let row = sqlx::query_as::<_, (String, Json<serde_json::Value>)>(
        "SELECT status,data FROM engine_job_runs WHERE id=$1 AND tenant_id=$2 AND source_id=$3 \
         AND subject_id=$4 FOR UPDATE",
    )
    .bind(&generation.job_run_id)
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(subject_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "subject memory generation job disappeared during final commit".to_string())?;
    let (stored_status, data) = row;
    let mut job: EngineJobRun = decode(data)?;
    if stored_status != "running" || job.status != "running" {
        return Err("subject memory generation job is not running".to_string());
    }
    job.status = result.status;
    job.input_count = result.input_count;
    job.output_count = result.output_count;
    job.processed_count = result.processed_count;
    job.success_count = result.success_count;
    job.error_count = result.error_count;
    job.metadata = result.metadata;
    job.error_message = result.error_message;
    job.finished_at = Some(now.to_string());
    let updated = sqlx::query(
        "UPDATE engine_job_runs SET status=$2,finished_at=$3,data=$4 WHERE id=$1 AND status='running'",
    )
    .bind(&job.id)
    .bind(&job.status)
    .bind(timestamp(now)?)
    .bind(json(&job)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() != 1 {
        return Err("subject memory generation job changed during final commit".to_string());
    }
    Ok(())
}

fn validate_l0_finalization(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
    finalization: &ThreadL0GenerationFinalization<'_>,
) -> Result<(), String> {
    if generation.generation_kind != "thread_summary" || output.level != 0 {
        return Err("only a thread L0 generation can finalize source records".to_string());
    }
    if finalization.record_ids.is_empty()
        || finalization.record_ids.len() != output.source_record_count
    {
        return Err("thread L0 finalization record count does not match output".to_string());
    }
    let unique = finalization
        .record_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != finalization.record_ids.len()
        || unique.iter().any(|value| value.trim().is_empty())
    {
        return Err("thread L0 finalization record ids must be unique and non-empty".to_string());
    }
    let frozen_ids = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "thread L0 generation input manifest has no inputs".to_string())?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "thread L0 generation input manifest has an invalid id".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if frozen_ids.as_slice()
        != finalization
            .record_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
    {
        return Err("thread L0 finalization records do not match frozen input order".to_string());
    }
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err("thread L0 finalization requires a successful terminal job result".to_string());
    }
    Ok(())
}

fn validate_resummarization_finalization(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
    finalization: &ThreadL0ResummarizationFinalization<'_>,
) -> Result<(), String> {
    if generation.generation_kind != "thread_summary"
        || output.level != 0
        || output.generation_reason != "resummarize"
    {
        return Err(
            "covered-range resummarization requires a thread L0 resummarize generation".to_string(),
        );
    }
    if finalization.previous_summary_id.trim().is_empty()
        || finalization.record_ids.is_empty()
        || finalization.record_ids.len() != output.source_record_count
    {
        return Err(
            "covered-range resummarization identity or record count is invalid".to_string(),
        );
    }
    let unique = finalization
        .record_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != finalization.record_ids.len()
        || unique.iter().any(|value| value.trim().is_empty())
    {
        return Err(
            "covered-range resummarization record ids must be unique and non-empty".to_string(),
        );
    }
    validate_frozen_source_ids(generation, finalization.record_ids, "resummarization")?;
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err(
            "covered-range resummarization requires a successful terminal job result".to_string(),
        );
    }
    Ok(())
}

async fn supersede_resummarized_range(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    replacement: &EngineSummary,
    project_partition_key: &str,
    finalization: &ThreadL0ResummarizationFinalization<'_>,
) -> Result<(), String> {
    let claim = sqlx::query_scalar::<_, String>(
        "SELECT lease_token FROM engine_covered_range_resummarization_claims \
         WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 AND job_run_id=$4 \
           AND status='running' AND lease_expires_at>now() FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(&generation.job_run_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if claim.is_none() {
        return Err("covered-range resummarization claim is missing or expired".to_string());
    }
    let previous = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND thread_id=$4 AND project_partition_key=$5 AND level=0 AND status='done' \
         AND recall_eligible FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(&replacement.thread_id)
    .bind(project_partition_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "covered-range source summary is no longer active".to_string())?;
    let mut previous: EngineSummary = decode(previous)?;

    let covered_ids = sqlx::query_scalar::<_, String>(
        "SELECT record_id FROM engine_summary_record_coverage WHERE tenant_id=$1 AND source_id=$2 \
         AND summary_id=$3 AND status='active' ORDER BY ordinal FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if covered_ids != finalization.record_ids {
        return Err(
            "covered-range active coverage changed after generation was frozen".to_string(),
        );
    }

    let owned_records: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND project_partition_key=$4 AND id=ANY($5) AND summary_status='summarized' \
         AND summary_id=$6 AND recall_eligible FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&replacement.thread_id)
    .bind(project_partition_key)
    .bind(finalization.record_ids)
    .bind(finalization.previous_summary_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if owned_records.len() != finalization.record_ids.len() {
        return Err("covered-range source records changed after generation was frozen".to_string());
    }

    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_summary_record_coverage SET status='superseded',retired_at=$4, \
         retired_reason='covered_range_resummarized' WHERE tenant_id=$1 AND source_id=$2 \
         AND summary_id=$3 AND status='active'",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;

    previous.status = "superseded".to_string();
    previous.updated_at = now.clone();
    let retired = sqlx::query(
        "UPDATE engine_summaries SET status='superseded',recall_eligible=false,retired_at=$4, \
         retired_reason='covered_range_resummarized',rollup_dispatch_pending=false, \
         subject_memory_source_dispatch_pending=false,updated_at=$4,data=$5 \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='done' AND recall_eligible",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .bind(json(&previous)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if retired.rows_affected() != 1 {
        return Err("covered-range source summary changed during replacement".to_string());
    }

    // Every active descendant is invalid once its source range changes. Retire the complete
    // downstream frontier in this transaction; the replacement summary's durable outbox events
    // rebuild it from the new canonical generation.
    sqlx::query(
        "WITH RECURSIVE affected(output_kind,output_id) AS ( \
           SELECT output_kind,output_id FROM engine_memory_derivation_edges \
            WHERE tenant_id=$1 AND source_id=$2 AND input_kind='thread_summary' AND input_id=$3 \
              AND status='active' \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.output_kind AND edge.input_id=input.output_id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
         ) UPDATE engine_summaries summary SET status='retired',recall_eligible=false, \
             retired_at=$4,retired_reason='upstream_resummarized',updated_at=$4, \
             rollup_dispatch_pending=false,subject_memory_source_dispatch_pending=false, \
             data=jsonb_set(data,'{status}',to_jsonb('retired'::text),true) \
          FROM affected WHERE affected.output_kind='thread_summary' \
            AND summary.tenant_id=$1 AND summary.source_id=$2 AND summary.id=affected.output_id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "WITH RECURSIVE affected(output_kind,output_id) AS ( \
           SELECT output_kind,output_id FROM engine_memory_derivation_edges \
            WHERE tenant_id=$1 AND source_id=$2 AND input_kind='thread_summary' AND input_id=$3 \
              AND status='active' \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.output_kind AND edge.input_id=input.output_id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
         ) UPDATE engine_subject_memories memory SET status='retired',recall_eligible=false, \
             retired_at=$4,retired_reason='upstream_resummarized',updated_at=$4, \
             data=jsonb_set(data,'{status}',to_jsonb('retired'::text),true) \
          FROM affected WHERE affected.output_kind='subject_memory' \
            AND memory.tenant_id=$1 AND memory.source_id=$2 AND memory.id=affected.output_id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "WITH RECURSIVE affected(kind,id) AS ( \
           SELECT 'thread_summary'::text,$3::text \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.kind AND edge.input_id=input.id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
         ) UPDATE engine_memory_derivation_edges edge SET status='superseded',retired_at=$4, \
             retired_reason='upstream_resummarized' FROM affected input \
          WHERE edge.tenant_id=$1 AND edge.source_id=$2 AND edge.status='active' \
            AND edge.input_kind=input.kind AND edge.input_id=input.id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "WITH RECURSIVE affected(kind,id) AS ( \
           SELECT 'thread_summary'::text,$3::text \
           UNION \
           SELECT edge.output_kind,edge.output_id FROM engine_memory_derivation_edges edge \
           JOIN affected input ON edge.input_kind=input.kind AND edge.input_id=input.id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 \
              AND edge.status IN ('active','superseded') \
         ) UPDATE engine_memory_fact_projections fact SET status='retired',retired_at=$4 \
           FROM affected WHERE fact.tenant_id=$1 AND fact.source_id=$2 \
             AND fact.status IN ('active','uncertain') \
             AND fact.output_kind=affected.kind AND fact.output_id=affected.id",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn finalize_thread_l0_resummarization(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    summary: &EngineSummary,
    project_partition_key: &str,
    finalization: ThreadL0ResummarizationFinalization<'_>,
) -> Result<(), String> {
    let now = now_rfc3339();
    for record_id in finalization.record_ids {
        let row = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
             AND thread_id=$4 AND project_partition_key=$5 AND summary_status='summarized' \
             AND summary_id=$6 AND recall_eligible FOR UPDATE",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(record_id)
        .bind(&summary.thread_id)
        .bind(project_partition_key)
        .bind(finalization.previous_summary_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "covered-range source record changed before replacement commit".to_string()
        })?;
        let mut record: EngineRecord = decode(row)?;
        record.summary_id = Some(summary.id.clone());
        record.summarized_at = Some(now.clone());
        sqlx::query(
            "UPDATE engine_records SET summary_id=$4,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_id=$6 \
               AND summary_status='summarized' AND recall_eligible",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(record_id)
        .bind(&summary.id)
        .bind(json(&record)?)
        .bind(finalization.previous_summary_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    lineage::insert_thread_l0_coverage(
        tx,
        generation,
        summary,
        project_partition_key,
        finalization.record_ids,
    )
    .await?;
    let completed = sqlx::query(
        "UPDATE engine_covered_range_resummarization_claims SET status='completed', \
         completed_at=$5,updated_at=$5 WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 \
         AND job_run_id=$4 AND status='running'",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(finalization.previous_summary_id)
    .bind(&generation.job_run_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if completed.rows_affected() != 1 {
        return Err("covered-range resummarization claim changed before commit".to_string());
    }
    finish_generation_job(
        tx,
        generation,
        &summary.thread_id,
        &summary.id,
        finalization.job_result,
        &now,
    )
    .await
}

fn validate_rollup_finalization(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
    finalization: &ThreadRollupGenerationFinalization<'_>,
) -> Result<(), String> {
    if generation.generation_kind != "thread_rollup" || output.level <= 0 {
        return Err("only a thread rollup generation can finalize source summaries".to_string());
    }
    if finalization.summary_ids.is_empty()
        || finalization.summary_ids.len() != output.source_record_count
    {
        return Err("thread rollup finalization source count does not match output".to_string());
    }
    let unique = finalization
        .summary_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != finalization.summary_ids.len()
        || unique.iter().any(|value| value.trim().is_empty())
    {
        return Err("thread rollup source ids must be unique and non-empty".to_string());
    }
    let frozen_ids = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "thread rollup generation input manifest has no inputs".to_string())?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    "thread rollup generation input manifest has an invalid id".to_string()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let requested_ids = finalization
        .summary_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    if frozen_ids != requested_ids {
        return Err("thread rollup sources do not match frozen input order".to_string());
    }
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err(
            "thread rollup finalization requires a successful terminal job result".to_string(),
        );
    }
    Ok(())
}

async fn finalize_thread_l0_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    summary: &EngineSummary,
    project_partition_key: &str,
    finalization: ThreadL0GenerationFinalization<'_>,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND id=ANY($4) AND project_partition_key=$5 AND summary_status='summarizing' \
         AND summary_job_run_id=$6 FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&summary.thread_id)
    .bind(finalization.record_ids)
    .bind(project_partition_key)
    .bind(&generation.job_run_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != finalization.record_ids.len() {
        return Err("thread L0 finalization did not own every frozen source record".to_string());
    }
    let now = now_rfc3339();
    for row in rows {
        let mut record: EngineRecord = decode(row)?;
        record.summary_status = "summarized".to_string();
        record.summary_id = Some(summary.id.clone());
        record.summarized_at = Some(now.clone());
        let updated = sqlx::query(
            "UPDATE engine_records SET summary_status='summarized',summary_id=$4, \
             summary_job_run_id=NULL,summary_started_at=NULL,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_status='summarizing' \
             AND summary_job_run_id=$6",
        )
        .bind(&record.tenant_id)
        .bind(&record.source_id)
        .bind(&record.id)
        .bind(&summary.id)
        .bind(json(&record)?)
        .bind(&generation.job_run_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("thread L0 source record claim changed during commit".to_string());
        }
    }

    lineage::insert_thread_l0_coverage(
        tx,
        generation,
        summary,
        project_partition_key,
        finalization.record_ids,
    )
    .await?;

    let partition = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         pending_record_count=GREATEST(pending_record_count-$6,0), \
         pending_summary_tokens=GREATEST(pending_summary_tokens-$7,0), \
         summary_status=CASE WHEN GREATEST(pending_record_count-$6,0)>0 THEN 'pending' ELSE 'idle' END, \
         summary_job_run_id=NULL,summary_locked_at=NULL,summary_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND ((summary_status='running' AND summary_job_run_id=$5) OR \
           (summary_status<>'running' AND (summary_job_run_id IS NULL OR summary_job_run_id=$5))) \
         AND pending_record_count>=$6 AND pending_summary_tokens>=$7",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&summary.thread_id)
    .bind(project_partition_key)
    .bind(&generation.job_run_id)
    .bind(finalization.record_ids.len() as i64)
    .bind(finalization.consumed_summary_tokens.max(0))
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if partition.rows_affected() != 1 {
        return Err("thread L0 partition claim was lost before final commit".to_string());
    }

    finish_generation_job(
        tx,
        generation,
        &summary.thread_id,
        &summary.id,
        finalization.job_result,
        &now,
    )
    .await
}

async fn finalize_thread_rollup_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSummary,
    project_partition_key: &str,
    finalization: ThreadRollupGenerationFinalization<'_>,
) -> Result<(), String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND id=ANY($4) AND project_partition_key=$5 AND status='done' AND recall_eligible \
         AND rollup_status='pending' FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&output.thread_id)
    .bind(finalization.summary_ids)
    .bind(project_partition_key)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != finalization.summary_ids.len() {
        return Err(
            "thread rollup finalization did not own every frozen source summary".to_string(),
        );
    }
    let now = now_rfc3339();
    for row in rows {
        let mut source: EngineSummary = decode(row)?;
        source.rollup_status = "done".to_string();
        source.rollup_summary_id = Some(output.id.clone());
        source.rolled_up_at = Some(now.clone());
        source.updated_at = now.clone();
        let updated = sqlx::query(
            "UPDATE engine_summaries SET rollup_status='done',rollup_dispatch_pending=false, \
             rollup_dispatch_consumed_version=GREATEST(rollup_dispatch_consumed_version, \
             rollup_dispatch_version),rollup_dispatch_consumed_at=$4,updated_at=$4,data=$5 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND rollup_status='pending'",
        )
        .bind(&source.tenant_id)
        .bind(&source.source_id)
        .bind(&source.id)
        .bind(timestamp(&now)?)
        .bind(json(&source)?)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
        if updated.rows_affected() != 1 {
            return Err("thread rollup source changed during final commit".to_string());
        }
    }

    lineage::insert_thread_rollup_edges(
        tx,
        generation,
        output,
        project_partition_key,
        finalization.summary_ids,
    )
    .await?;

    let released = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET rollup_status='pending', \
         rollup_job_run_id=NULL,rollup_locked_at=NULL,rollup_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND ((rollup_status='running' AND rollup_job_run_id=$5) OR \
           (rollup_status<>'running' AND (rollup_job_run_id IS NULL OR rollup_job_run_id=$5)))",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&output.thread_id)
    .bind(project_partition_key)
    .bind(&generation.job_run_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if released.rows_affected() != 1 {
        return Err("thread rollup partition claim was lost before final commit".to_string());
    }
    finish_generation_job(
        tx,
        generation,
        &output.thread_id,
        &output.id,
        finalization.job_result,
        &now,
    )
    .await
}

async fn lock_running_generation_job(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    expected_thread_id: Option<&str>,
    expected_subject_id: Option<&str>,
) -> Result<(), String> {
    let row = sqlx::query_as::<_, (String, Json<serde_json::Value>)>(
        "SELECT status,data FROM engine_job_runs \
         WHERE id=$1 AND tenant_id=$2 AND source_id=$3 FOR UPDATE",
    )
    .bind(&generation.job_run_id)
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "generation job disappeared before final commit".to_string())?;
    let (stored_status, data) = row;
    let job: EngineJobRun = decode(data)?;
    if stored_status != "running"
        || job.status != "running"
        || expected_thread_id.is_some_and(|thread_id| job.thread_id.as_deref() != Some(thread_id))
        || expected_subject_id
            .is_some_and(|subject_id| job.subject_id.as_deref() != Some(subject_id))
    {
        return Err("generation job is not the running owner of the final output".to_string());
    }
    Ok(())
}

async fn finish_generation_job(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    thread_id: &str,
    output_id: &str,
    result: FinishEngineJobRunRequest,
    now: &str,
) -> Result<(), String> {
    let row = sqlx::query_as::<_, (String, Json<serde_json::Value>)>(
        "SELECT status,data FROM engine_job_runs \
         WHERE id=$1 AND tenant_id=$2 AND source_id=$3 FOR UPDATE",
    )
    .bind(&generation.job_run_id)
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "generation job disappeared during final commit".to_string())?;
    let (stored_status, data) = row;
    let mut job: EngineJobRun = decode(data)?;
    if stored_status != "running"
        || job.status != "running"
        || job.thread_id.as_deref() != Some(thread_id)
    {
        return Err("generation job is not the running owner of the final output".to_string());
    }
    job.status = result.status;
    job.input_count = result.input_count;
    job.output_count = result.output_count;
    job.processed_count = result.processed_count;
    job.success_count = result.success_count;
    job.error_count = result.error_count;
    job.metadata = result.metadata;
    if let Some(metadata) = job
        .metadata
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    {
        metadata.insert(
            "generated_summary_id".to_string(),
            serde_json::Value::String(output_id.to_string()),
        );
    }
    job.error_message = result.error_message;
    job.finished_at = Some(now.to_string());
    sqlx::query("UPDATE engine_job_runs SET status=$2,finished_at=$3,data=$4 WHERE id=$1")
        .bind(&job.id)
        .bind(&job.status)
        .bind(timestamp(now)?)
        .bind(json(&job)?)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn validate_summary_output(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
) -> Result<(), String> {
    let expected_kind = if output.level == 0 {
        "thread_summary"
    } else {
        "thread_rollup"
    };
    if generation.generation_kind != expected_kind {
        return Err(format!(
            "generation kind cannot produce summary level {}",
            output.level
        ));
    }
    if output.thread_id.trim().is_empty()
        || output.subject_id.trim().is_empty()
        || output.summary_type.trim().is_empty()
        || output.summary_text.trim().is_empty()
        || output.generation_reason.trim().is_empty()
        || output.generation_profile.trim().is_empty()
        || output.pipeline_revision.trim().is_empty()
    {
        return Err("thread summary generation output identity is incomplete".to_string());
    }
    if output.level < 0 {
        return Err("thread summary generation output level cannot be negative".to_string());
    }
    if output.summary_type == "thread_noop"
        && (output.level != 0 || generation.generation_kind != "thread_summary")
    {
        return Err(
            "thread no-op output is only valid for a level-0 thread generation".to_string(),
        );
    }
    if generation.project_partition_key.as_deref() != Some(output.project_partition_key) {
        return Err("thread summary output partition does not match generation".to_string());
    }
    if output.project_partition_key != "__none__"
        && !output.project_partition_key.starts_with("project:")
    {
        return Err("thread summary output partition is invalid".to_string());
    }
    if generation
        .config_snapshot
        .get("generation_profile")
        .and_then(serde_json::Value::as_str)
        != Some(output.generation_profile)
        || generation
            .config_snapshot
            .get("pipeline_revision")
            .and_then(serde_json::Value::as_str)
            != Some(output.pipeline_revision)
    {
        return Err(
            "thread summary output configuration does not match frozen generation".to_string(),
        );
    }
    if output.payload_schema != crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA {
        return Err("thread summary output payload_schema is invalid".to_string());
    }
    Ok(())
}

async fn load_committed_thread_summary(
    db: &Db,
    generation: &GenerationCommit,
) -> Result<EngineSummary, String> {
    let stored = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "generation disappeared during idempotent commit".to_string())?;
    if stored.status != "committed" || stored.output_kind.as_deref() != Some("thread_summary") {
        return Err("generation commit lease was lost before summary output committed".to_string());
    }
    let output_id = stored
        .output_id
        .as_deref()
        .ok_or_else(|| "committed summary generation is missing output_id".to_string())?;
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND generation_id=$4",
    )
    .bind(&stored.tenant_id)
    .bind(&stored.source_id)
    .bind(output_id)
    .bind(&stored.generation_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "committed summary generation output is missing".to_string())
    .and_then(decode)
}

fn validate_subject_output(
    generation: &GenerationCommit,
    output: &SubjectMemoryGenerationOutput<'_>,
) -> Result<(), String> {
    if !matches!(
        generation.generation_kind.as_str(),
        "project_memory" | "agent_memory" | "subject_memory_rollup"
    ) {
        return Err("generation kind cannot produce a subject memory".to_string());
    }
    if output.request.tenant_id != generation.tenant_id
        || output.request.source_id != generation.source_id
    {
        return Err("subject memory output scope does not match generation".to_string());
    }
    if output.subject_id.trim().is_empty()
        || output.memory_key.trim().is_empty()
        || output.generation_profile.trim().is_empty()
        || output.pipeline_revision.trim().is_empty()
    {
        return Err("subject memory generation output identity is incomplete".to_string());
    }
    if generation.project_partition_key.as_deref() != output.project_partition_key {
        return Err("subject memory output partition does not match generation".to_string());
    }
    if output.request.status.as_deref().unwrap_or("active") != "active" {
        return Err("subject memory generation output must be active".to_string());
    }
    if generation
        .config_snapshot
        .get("generation_profile")
        .and_then(serde_json::Value::as_str)
        != Some(output.generation_profile)
        || generation
            .config_snapshot
            .get("pipeline_revision")
            .and_then(serde_json::Value::as_str)
            != Some(output.pipeline_revision)
    {
        return Err(
            "subject memory output configuration does not match frozen generation".to_string(),
        );
    }
    if output.payload_schema != crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA {
        return Err("subject memory output payload_schema is invalid".to_string());
    }
    Ok(())
}

async fn load_committed_subject_memory(
    db: &Db,
    generation: &GenerationCommit,
) -> Result<EngineSubjectMemory, String> {
    let stored = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "generation disappeared during idempotent commit".to_string())?;
    if stored.status != "committed" || stored.output_kind.as_deref() != Some("subject_memory") {
        return Err("generation commit lease was lost before subject output committed".to_string());
    }
    let output_id = stored
        .output_id
        .as_deref()
        .ok_or_else(|| "committed subject generation is missing output_id".to_string())?;
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND generation_id=$4",
    )
    .bind(&stored.tenant_id)
    .bind(&stored.source_id)
    .bind(output_id)
    .bind(&stored.generation_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "committed subject generation output is missing".to_string())
    .and_then(decode)
}
