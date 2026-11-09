// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrivilegedThreadErasureResult {
    pub erasure_id: String,
    pub deleted_edges: u64,
    pub deleted_coverage: u64,
    pub deleted_summaries: u64,
    pub deleted_memories: u64,
    pub deleted_records: u64,
    pub deleted_projections: u64,
    pub deleted_generations: u64,
    pub deleted_jobs: u64,
    pub deleted_subjects: u64,
    pub deleted_thread: bool,
}

pub async fn erase_thread(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    reason_code: &str,
) -> Result<PrivilegedThreadErasureResult, String> {
    validate_required("tenant_id", tenant_id)?;
    validate_required("source_id", source_id)?;
    validate_required("thread_id", thread_id)?;
    if !matches!(
        reason_code,
        "user_delete" | "tenant_delete" | "legal_erasure" | "retention_expired"
    ) {
        return Err("unsupported privileged erasure reason_code".to_string());
    }

    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let thread_subject_id = sqlx::query_scalar::<_, String>(
        "SELECT subject_id FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let Some(thread_subject_id) = thread_subject_id else {
        return Err("thread not found for privileged erasure".to_string());
    };

    let doomed = sqlx::query_as::<_, (String, String)>(
        "WITH RECURSIVE doomed(kind,id) AS ( \
           SELECT 'thread_summary'::TEXT,id FROM engine_summaries \
            WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
           UNION \
           SELECT edge.output_kind,edge.output_id \
             FROM engine_memory_derivation_edges edge \
             JOIN doomed input ON input.kind=edge.input_kind AND input.id=edge.input_id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 \
         ) SELECT kind,id FROM doomed",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let summary_ids = doomed
        .iter()
        .filter(|(kind, _)| kind == "thread_summary")
        .map(|(_, id)| id.clone())
        .collect::<Vec<_>>();
    let memory_ids = doomed
        .iter()
        .filter(|(kind, _)| kind == "subject_memory")
        .map(|(_, id)| id.clone())
        .collect::<Vec<_>>();

    let memory_subject_ids = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT subject_id FROM engine_subject_memories \
         WHERE tenant_id=$1 AND source_id=$2 AND id=ANY($3)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let generation_rows = sqlx::query_as::<_, (String, String)>(
        "SELECT generation_id,job_run_id FROM engine_generation_commits \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id IN ( \
           SELECT generation_id FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 \
             AND id=ANY($3) AND generation_id IS NOT NULL \
           UNION \
           SELECT generation_id FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
             AND id=ANY($4) AND generation_id IS NOT NULL \
         ) FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&summary_ids)
    .bind(&memory_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let generation_ids = generation_rows
        .iter()
        .map(|(generation_id, _)| generation_id.clone())
        .collect::<Vec<_>>();
    let mut job_ids = generation_rows
        .iter()
        .map(|(_, job_id)| job_id.clone())
        .collect::<Vec<_>>();
    job_ids.extend(
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM engine_job_runs WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3",
        )
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| error.to_string())?,
    );
    job_ids.sort();
    job_ids.dedup();

    let agent_run_ids = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT data->>'cloud_agent_run_id' FROM engine_job_runs \
         WHERE id=ANY($1) AND NULLIF(BTRIM(data->>'cloud_agent_run_id'),'') IS NOT NULL",
    )
    .bind(&job_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        "DELETE FROM engine_memory_fact_supersessions edge WHERE edge.tenant_id=$1 \
         AND edge.source_id=$2 AND (edge.newer_fact_id IN ( \
           SELECT fact_id FROM engine_memory_fact_projections WHERE tenant_id=$1 AND source_id=$2 \
            AND ((output_kind='thread_summary' AND output_id=ANY($3)) \
              OR (output_kind='subject_memory' AND output_id=ANY($4)))) \
         OR edge.superseded_fact_id IN ( \
           SELECT fact_id FROM engine_memory_fact_projections WHERE tenant_id=$1 AND source_id=$2 \
            AND ((output_kind='thread_summary' AND output_id=ANY($3)) \
              OR (output_kind='subject_memory' AND output_id=ANY($4)))))",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&summary_ids)
    .bind(&memory_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let deleted_fact_projections = sqlx::query(
        "DELETE FROM engine_memory_fact_projections WHERE tenant_id=$1 AND source_id=$2 AND ( \
         (output_kind='thread_summary' AND output_id=ANY($3)) OR \
         (output_kind='subject_memory' AND output_id=ANY($4)))",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&summary_ids)
    .bind(&memory_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    sqlx::query(
        "DELETE FROM engine_derived_event_outbox WHERE tenant_id=$1 AND source_id=$2 AND ( \
           generation_id=ANY($3) OR aggregate_id=ANY($4) OR aggregate_id=ANY($5) \
           OR payload->>'thread_id'=$6)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&generation_ids)
    .bind(&summary_ids)
    .bind(&memory_ids)
    .bind(thread_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let deleted_edges = sqlx::query(
        "DELETE FROM engine_memory_derivation_edges WHERE tenant_id=$1 AND source_id=$2 \
         AND ((input_kind='thread_summary' AND input_id=ANY($3)) \
           OR (output_kind='thread_summary' AND output_id=ANY($3)) \
           OR (input_kind='subject_memory' AND input_id=ANY($4)) \
           OR (output_kind='subject_memory' AND output_id=ANY($4)))",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&summary_ids)
    .bind(&memory_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    let deleted_coverage = sqlx::query(
        "DELETE FROM engine_summary_record_coverage WHERE tenant_id=$1 AND source_id=$2 \
         AND (thread_id=$3 OR summary_id=ANY($4))",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(&summary_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    let deleted_summaries = sqlx::query(
        "DELETE FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=ANY($3)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&summary_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let deleted_memories = sqlx::query(
        "DELETE FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 AND id=ANY($3)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let deleted_records = sqlx::query(
        "DELETE FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    let mut deleted_projections = deleted_fact_projections;
    for table in [
        "engine_compact_turns",
        "engine_thread_snapshots",
        "engine_thread_summary_partitions",
    ] {
        let sql =
            format!("DELETE FROM {table} WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3");
        deleted_projections += sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(tenant_id)
            .bind(source_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?
            .rows_affected();
    }

    let deleted_generations = sqlx::query(
        "DELETE FROM engine_generation_commits WHERE tenant_id=$1 AND source_id=$2 \
         AND generation_id=ANY($3)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&generation_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    let deleted_jobs = sqlx::query(
        "DELETE FROM engine_job_runs job WHERE id=ANY($1) \
         AND NOT EXISTS(SELECT 1 FROM engine_generation_commits generation \
                        WHERE generation.job_run_id=job.id)",
    )
    .bind(&job_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    sqlx::query("DELETE FROM cloud_agent_runs WHERE agent_run_id=ANY($1)")
        .bind(&agent_run_ids)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

    let deleted_thread =
        sqlx::query("DELETE FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3")
            .bind(tenant_id)
            .bind(source_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?
            .rows_affected()
            > 0;

    let mut subject_ids = memory_subject_ids;
    subject_ids.push(thread_subject_id);
    subject_ids.sort();
    subject_ids.dedup();
    let deleted_subjects = sqlx::query(
        "DELETE FROM engine_subjects subject WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=ANY($3) \
         AND NOT EXISTS(SELECT 1 FROM engine_threads thread WHERE thread.tenant_id=$1 \
                        AND thread.source_id=$2 AND thread.subject_id=subject.subject_id) \
         AND NOT EXISTS(SELECT 1 FROM engine_subject_memories memory WHERE memory.tenant_id=$1 \
                        AND memory.source_id=$2 AND memory.subject_id=subject.subject_id) \
         AND NOT EXISTS(SELECT 1 FROM engine_subject_memory_scopes scope WHERE scope.tenant_id=$1 \
                        AND scope.source_id=$2 AND scope.subject_id=subject.subject_id)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&subject_ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    let erasure_id = format!("erase_{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO engine_privileged_erasure_audits( \
           erasure_id,erasure_kind,reason_code,deleted_edges,deleted_coverage,deleted_summaries, \
           deleted_memories,deleted_records,deleted_projections,deleted_generations,deleted_jobs, \
           deleted_subjects,deleted_thread) \
         VALUES($1,'thread',$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(&erasure_id)
    .bind(reason_code)
    .bind(deleted_edges as i64)
    .bind(deleted_coverage as i64)
    .bind(deleted_summaries as i64)
    .bind(deleted_memories as i64)
    .bind(deleted_records as i64)
    .bind(deleted_projections as i64)
    .bind(deleted_generations as i64)
    .bind(deleted_jobs as i64)
    .bind(deleted_subjects as i64)
    .bind(i64::from(deleted_thread))
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(PrivilegedThreadErasureResult {
        erasure_id,
        deleted_edges,
        deleted_coverage,
        deleted_summaries,
        deleted_memories,
        deleted_records,
        deleted_projections,
        deleted_generations,
        deleted_jobs,
        deleted_subjects,
        deleted_thread,
    })
}

fn validate_required(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(())
    }
}
