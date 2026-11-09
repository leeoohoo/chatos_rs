// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubjectMemory, EngineSummary, UpsertSubjectMemoryRequest};
use crate::repositories::derived_events;
use crate::repositories::generation_commits::{self, GenerationCommit};
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

pub async fn commit_thread_summary_generation(
    db: &Db,
    generation: &GenerationCommit,
    output: ThreadSummaryGenerationOutput<'_>,
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
        output.project_partition_key,
        &current.generation_id,
    )
    .await?;
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
