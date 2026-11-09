// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use sqlx::types::Json;
use sqlx::{Postgres, Transaction};

use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineJobRun, EngineRecord, EngineSubjectMemory, EngineSummary, EngineThread,
    FinishEngineJobRunRequest, UpsertSubjectMemoryRequest,
};
use crate::repositories::derived_events;
use crate::repositories::generation_commits::{self, GenerationCommit};
use crate::repositories::postgres::{decode, json, optional_timestamp, timestamp};
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
    pub payload_schema: Option<&'a str>,
    pub structured_payload: Option<&'a serde_json::Value>,
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
    pub payload_schema: Option<&'a str>,
    pub structured_payload: Option<&'a serde_json::Value>,
    pub pipeline_revision: &'a str,
}

pub struct ThreadL0GenerationFinalization<'a> {
    pub record_ids: &'a [String],
    pub consumed_summary_tokens: i64,
    pub job_result: FinishEngineJobRunRequest,
}

pub struct ThreadRollupGenerationFinalization<'a> {
    pub summary_ids: &'a [String],
    pub job_result: FinishEngineJobRunRequest,
}

pub async fn commit_thread_summary_generation(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, None).await
}

pub async fn commit_thread_summary_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadL0GenerationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, Some(finalization), None).await
}

pub async fn commit_thread_rollup_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    finalization: ThreadRollupGenerationFinalization<'_>,
) -> Result<EngineSummary, String> {
    commit_thread_summary_generation_inner(db, generation, output, None, Some(finalization)).await
}

async fn commit_thread_summary_generation_inner(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
    l0_finalization: Option<ThreadL0GenerationFinalization<'_>>,
    rollup_finalization: Option<ThreadRollupGenerationFinalization<'_>>,
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

    let project_partition_key = output.project_partition_key;
    let summary = new_summary(
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
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type, \
         level,source_digest,status,rollup_status,subject_memory_summarized,rollup_dispatch_pending, \
         rollup_dispatch_version,rollup_dispatch_requested_at,subject_memory_source_dispatch_pending, \
         subject_memory_source_dispatch_version,subject_memory_source_dispatch_requested_at, \
         created_at,updated_at,data,generation_id,generation_reason,generation_profile, \
         project_partition_key,recall_eligible,payload_schema,structured_payload,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,true,1,$12,true,1,$12,$12,$13,$14, \
         $15,$16,$17,$18,true,$19,$20,$21)",
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
    .bind(timestamp(&summary.created_at)?)
    .bind(timestamp(&summary.updated_at)?)
    .bind(json(&summary)?)
    .bind(&current.generation_id)
    .bind(output.generation_reason)
    .bind(output.generation_profile)
    .bind(output.project_partition_key)
    .bind(output.payload_schema)
    .bind(output.structured_payload.map(Json))
    .bind(output.pipeline_revision)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
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
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
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
    }

    let memory = build_subject_memory(
        None,
        output.subject_id,
        output.memory_key,
        output.request,
        Some(output.source_digest),
        Some(output.rollup_status),
    );
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
    .bind(output.structured_payload.map(Json))
    .bind(output.pipeline_revision)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
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

    let partition = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         pending_record_count=GREATEST(pending_record_count-$6,0), \
         pending_summary_tokens=GREATEST(pending_summary_tokens-$7,0), \
         summary_status=CASE WHEN GREATEST(pending_record_count-$6,0)>0 THEN 'pending' ELSE 'idle' END, \
         summary_job_run_id=NULL,summary_locked_at=NULL,summary_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND authoritative_writer='v2' AND summary_status='running' AND summary_job_run_id=$5 \
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

    update_thread_summary_projection(
        tx,
        generation,
        &summary.thread_id,
        finalization.record_ids.len() as i64,
        finalization.consumed_summary_tokens,
        &now,
    )
    .await?;
    finish_generation_job(
        tx,
        generation,
        &summary.thread_id,
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

    let released = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET rollup_status='pending', \
         rollup_job_run_id=NULL,rollup_locked_at=NULL,rollup_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND authoritative_writer='v2' AND rollup_status='running' AND rollup_job_run_id=$5",
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
    sqlx::query(
        "UPDATE engine_threads SET rollup_job_run_id=NULL,rollup_locked_at=NULL, \
         rollup_lock_expires_at=NULL WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND rollup_job_run_id=$4",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&output.thread_id)
    .bind(&generation.job_run_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    finish_generation_job(
        tx,
        generation,
        &output.thread_id,
        finalization.job_result,
        &now,
    )
    .await
}

async fn update_thread_summary_projection(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    thread_id: &str,
    consumed_record_count: i64,
    consumed_summary_tokens: i64,
    now: &str,
) -> Result<(), String> {
    let row = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(thread_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "thread disappeared during L0 finalization".to_string())?;
    let mut thread: EngineThread = decode(row)?;
    thread.pending_record_count =
        (thread.pending_record_count - consumed_record_count.max(0)).max(0);
    thread.pending_summary_tokens =
        (thread.pending_summary_tokens - consumed_summary_tokens.max(0)).max(0);
    let another_partition_running = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND summary_status='running')",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(thread_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if !another_partition_running {
        thread.summary_status = if thread.pending_record_count > 0 {
            "pending"
        } else {
            "idle"
        }
        .to_string();
        thread.summary_job_run_id = None;
        thread.summary_locked_at = None;
        thread.summary_lock_expires_at = None;
    }
    thread.updated_at = now.to_string();
    sqlx::query(
        "UPDATE engine_threads SET summary_status=$4,pending_record_count=$5, \
         pending_summary_tokens=$6,summary_job_run_id=$7,summary_locked_at=$8, \
         summary_lock_expires_at=$9,updated_at=$10,data=$11 \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(&thread.tenant_id)
    .bind(&thread.source_id)
    .bind(&thread.id)
    .bind(&thread.summary_status)
    .bind(thread.pending_record_count)
    .bind(thread.pending_summary_tokens)
    .bind(&thread.summary_job_run_id)
    .bind(optional_timestamp(thread.summary_locked_at.as_deref())?)
    .bind(optional_timestamp(
        thread.summary_lock_expires_at.as_deref(),
    )?)
    .bind(timestamp(now)?)
    .bind(json(&thread)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn finish_generation_job(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    thread_id: &str,
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
