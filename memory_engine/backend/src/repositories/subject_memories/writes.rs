// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::now_rfc3339;
use crate::repositories::postgres::timestamp;

pub async fn retire_subject_memory(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_key: &str,
) -> Result<usize, String> {
    for (field, value) in [
        ("tenant_id", tenant_id),
        ("source_id", source_id),
        ("subject_id", subject_id),
        ("memory_key", memory_key),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} is required to retire subject memory"));
        }
    }
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let memory_ids = sqlx::query_scalar::<_, String>(
        "WITH RECURSIVE retired(id) AS ( \
           SELECT id FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
             AND subject_id=$3 AND memory_key=$4 AND recall_eligible \
           UNION \
           SELECT edge.output_id FROM engine_memory_derivation_edges edge \
             JOIN retired input ON edge.input_kind='subject_memory' AND edge.input_id=input.id \
            WHERE edge.tenant_id=$1 AND edge.source_id=$2 \
              AND edge.output_kind='subject_memory' AND edge.status='active' \
         ) SELECT id FROM retired",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(subject_id)
    .bind(memory_key)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if memory_ids.is_empty() {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(0);
    }
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND id=ANY($3) FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let affected_summary_ids = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT input_id FROM engine_memory_derivation_edges \
         WHERE tenant_id=$1 AND source_id=$2 AND status='active' \
         AND input_kind='thread_summary' AND output_kind='subject_memory' AND output_id=ANY($3)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let affected_scope_keys = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT output_scope_key FROM engine_memory_derivation_edges \
         WHERE tenant_id=$1 AND source_id=$2 AND status='active' \
           AND output_kind='subject_memory' AND output_id=ANY($3)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_memory_derivation_edges SET status='retired',retired_at=$4, \
         retired_reason='memory_retired' WHERE tenant_id=$1 AND source_id=$2 AND status='active' \
         AND ((input_kind='subject_memory' AND input_id=ANY($3)) \
           OR (output_kind='subject_memory' AND output_id=ANY($3)))",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .bind(timestamp(&now)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_memory_fact_projections SET status='retired',retired_at=$4 \
         WHERE tenant_id=$1 AND source_id=$2 AND output_kind='subject_memory' \
         AND output_id=ANY($3) AND status IN ('active','uncertain')",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .bind(timestamp(&now)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if !affected_summary_ids.is_empty() {
        sqlx::query(
            "UPDATE engine_summaries SET subject_memory_summarized=0, \
             subject_memory_scope_keys='{}'::TEXT[],updated_at=$4 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=ANY($3) AND recall_eligible",
        )
        .bind(tenant_id)
        .bind(source_id)
        .bind(&affected_summary_ids)
        .bind(timestamp(&now)?)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    if !affected_scope_keys.is_empty() {
        sqlx::query(
            "UPDATE engine_subject_memory_scopes SET subject_memory_status='pending', \
             subject_memory_dispatch_version=subject_memory_dispatch_version+1, \
             subject_memory_dispatch_pending=true,subject_memory_dispatch_requested_at=$4, \
             subject_memory_dispatch_last_error=NULL,lock_owner=NULL,lock_expires_at=NULL,updated_at=$4 \
             WHERE tenant_id=$1 AND source_id=$2 AND scope_key=ANY($3) AND status='active'",
        )
        .bind(tenant_id)
        .bind(source_id)
        .bind(&affected_scope_keys)
        .bind(timestamp(&now)?)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    let retired = sqlx::query(
        "UPDATE engine_subject_memories SET status='retired',recall_eligible=false,retired_at=$4, \
         retired_reason='business_delete',updated_at=$4, \
         data=jsonb_set(jsonb_set(data,'{status}',to_jsonb('retired'::text),true), \
                        '{updated_at}',to_jsonb($5::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND id=ANY($3) AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&memory_ids)
    .bind(timestamp(&now)?)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected() as usize;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(retired)
}
