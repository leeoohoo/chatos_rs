// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSummary};
use crate::repositories::postgres::{decode, json, timestamp};
use sqlx::types::Json;

pub async fn mark_summaries_rolled_up(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_ids: &[String],
    rollup_summary_id: &str,
) -> Result<usize, String> {
    update(
        db,
        tenant_id,
        source_id,
        thread_id,
        summary_ids,
        "rollup_status='pending'",
        None,
        |s, now| {
            s.rollup_status = "done".to_string();
            s.rollup_summary_id = Some(rollup_summary_id.to_string());
            s.rolled_up_at = Some(now.to_string());
        },
        true,
    )
    .await
}
pub async fn mark_summaries_subject_memory_summarized(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_ids: &[String],
) -> Result<usize, String> {
    update(
        db,
        tenant_id,
        source_id,
        thread_id,
        summary_ids,
        "subject_memory_summarized<>1",
        None,
        |s, now| {
            s.subject_memory_summarized = 1;
            s.subject_memory_summarized_at = Some(now.to_string());
        },
        false,
    )
    .await
}
pub async fn mark_summaries_subject_memory_summarized_for_scope(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_ids: &[String],
    scope_key: &str,
) -> Result<usize, String> {
    let key = scope_key.trim();
    if key.is_empty() {
        return Ok(0);
    }
    update(
        db,
        tenant_id,
        source_id,
        thread_id,
        summary_ids,
        "NOT ($5=ANY(subject_memory_scope_keys))",
        Some(key),
        |s, now| {
            s.subject_memory_summarized = 1;
            s.subject_memory_summarized_at = Some(now.to_string());
        },
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn update<F>(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    ids: &[String],
    condition: &str,
    scope_key: Option<&str>,
    mutate: F,
    consume_rollup: bool,
) -> Result<usize, String>
where
    F: Fn(&mut EngineSummary, &str),
{
    if ids.is_empty() {
        return Ok(0);
    }
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let sql=format!("SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND id=ANY($4) AND {condition} FOR UPDATE");
    let mut q = sqlx::query_scalar::<_, Json<serde_json::Value>>(&sql)
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .bind(ids);
    if let Some(key) = scope_key {
        q = q.bind(key);
    }
    let rows = q.fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    let now = now_rfc3339();
    let mut count = 0;
    for row in rows {
        let mut summary: EngineSummary = decode(row)?;
        mutate(&mut summary, &now);
        summary.updated_at = now.clone();
        let result=if let Some(key)=scope_key{
            sqlx::query("UPDATE engine_summaries SET subject_memory_summarized=$2,subject_memory_scope_keys=array_append(subject_memory_scope_keys,$3),updated_at=$4,data=$5 WHERE id=$1")
                .bind(&summary.id).bind(summary.subject_memory_summarized).bind(key).bind(timestamp(&now)?).bind(json(&summary)?).execute(&mut *tx).await
        }else if consume_rollup{
            sqlx::query("UPDATE engine_summaries SET rollup_status=$2,rollup_dispatch_pending=false,rollup_dispatch_consumed_version=GREATEST(rollup_dispatch_consumed_version,rollup_dispatch_version),rollup_dispatch_consumed_at=$3,updated_at=$3,data=$4 WHERE id=$1")
                .bind(&summary.id).bind(&summary.rollup_status).bind(timestamp(&now)?).bind(json(&summary)?).execute(&mut *tx).await
        }else{
            sqlx::query("UPDATE engine_summaries SET subject_memory_summarized=$2,updated_at=$3,data=$4 WHERE id=$1")
                .bind(&summary.id).bind(summary.subject_memory_summarized).bind(timestamp(&now)?).bind(json(&summary)?).execute(&mut *tx).await
        }.map_err(|e|e.to_string())?;
        count += result.rows_affected() as usize;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(count)
}
