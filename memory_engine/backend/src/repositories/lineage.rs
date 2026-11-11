// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::models::{EngineSubjectMemory, EngineSummary};
use crate::repositories::generation_commits::GenerationCommit;

pub async fn insert_thread_l0_coverage(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    summary: &EngineSummary,
    project_partition_key: &str,
    record_ids: &[String],
) -> Result<(), String> {
    for (ordinal, record_id) in record_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO engine_summary_record_coverage( \
             id,tenant_id,source_id,thread_id,project_partition_key,summary_id,record_id,ordinal, \
             source_ref,coverage_kind,status,generation_id) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$7,'full','active',$9)",
        )
        .bind(format!("cov_{}", Uuid::new_v4()))
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(&summary.thread_id)
        .bind(project_partition_key)
        .bind(&summary.id)
        .bind(record_id)
        .bind(ordinal as i64)
        .bind(&generation.generation_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub async fn insert_thread_rollup_edges(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSummary,
    project_partition_key: &str,
    input_ids: &[String],
) -> Result<(), String> {
    let output_scope_key = format!(
        "thread:{}:partition:{}",
        output.thread_id, project_partition_key
    );
    insert_edges(
        tx,
        generation,
        "thread_rollup",
        output_scope_key.as_str(),
        "thread_summary",
        &output.id,
        "thread_summary",
        input_ids,
    )
    .await
}

pub async fn insert_subject_summary_edges(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSubjectMemory,
    input_ids: &[String],
) -> Result<(), String> {
    let derivation_kind = match generation.generation_kind.as_str() {
        "project_memory" => "project_memory",
        "agent_memory" => "agent_memory",
        value => {
            return Err(format!(
                "unsupported summary-to-memory derivation kind: {value}"
            ))
        }
    };
    let output_scope_key = format!(
        "subject:{}:memory:{}",
        output.subject_id, output.memory_type
    );
    insert_edges(
        tx,
        generation,
        derivation_kind,
        output_scope_key.as_str(),
        "subject_memory",
        &output.id,
        "thread_summary",
        input_ids,
    )
    .await
}

pub async fn insert_subject_memory_rollup_edges(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output: &EngineSubjectMemory,
    input_ids: &[String],
) -> Result<(), String> {
    let output_scope_key = format!(
        "subject:{}:memory:{}",
        output.subject_id, output.memory_type
    );
    insert_edges(
        tx,
        generation,
        "subject_memory_rollup",
        output_scope_key.as_str(),
        "subject_memory",
        &output.id,
        "subject_memory",
        input_ids,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn insert_edges(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    derivation_kind: &str,
    output_scope_key: &str,
    output_kind: &str,
    output_id: &str,
    input_kind: &str,
    input_ids: &[String],
) -> Result<(), String> {
    for (ordinal, input_id) in input_ids.iter().enumerate() {
        let table = match input_kind {
            "thread_summary" => "engine_summaries",
            "subject_memory" => "engine_subject_memories",
            _ => return Err(format!("unsupported derivation input kind: {input_kind}")),
        };
        let sql = format!(
            "SELECT project_partition_key FROM {table} \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3"
        );
        let input_partition = sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(sql))
            .bind(&generation.tenant_id)
            .bind(&generation.source_id)
            .bind(input_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "derivation input disappeared during final commit".to_string())?
            .ok_or_else(|| "derivation input has no canonical project partition".to_string())?;
        sqlx::query(
            "INSERT INTO engine_memory_derivation_edges( \
             id,tenant_id,source_id,derivation_kind,output_scope_key,input_project_partition_key, \
             output_kind,output_id,input_kind,input_id,ordinal,status,generation_id) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'active',$12)",
        )
        .bind(format!("edge_{}", Uuid::new_v4()))
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(derivation_kind)
        .bind(output_scope_key)
        .bind(input_partition)
        .bind(output_kind)
        .bind(output_id)
        .bind(input_kind)
        .bind(input_id)
        .bind(ordinal as i64)
        .bind(&generation.generation_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}
