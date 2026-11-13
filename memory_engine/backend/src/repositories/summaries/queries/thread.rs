// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::common::{build_summary_query, decode_summaries};
use crate::db::Db;
use crate::models::EngineSummary;
use sqlx::types::Json;

use super::super::ListSummariesQuery;

pub async fn get_summary_by_id(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<Option<EngineSummary>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(crate::repositories::postgres::decode)
    .transpose()
}

pub async fn list_latest_thread_summary_frontier(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_type: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT summary.data FROM engine_summaries summary \
         WHERE summary.tenant_id=$1 AND summary.source_id=$2 AND summary.thread_id=$3 \
           AND summary.summary_type=$4 AND summary.status='done' AND summary.recall_eligible \
           AND NOT EXISTS( \
             SELECT 1 FROM engine_memory_derivation_edges edge \
             JOIN engine_summaries output \
               ON output.tenant_id=edge.tenant_id AND output.source_id=edge.source_id \
              AND output.id=edge.output_id \
             WHERE edge.tenant_id=summary.tenant_id AND edge.source_id=summary.source_id \
               AND edge.status='active' AND edge.input_kind='thread_summary' \
               AND edge.input_id=summary.id AND edge.output_kind='thread_summary' \
               AND output.thread_id=summary.thread_id AND output.summary_type=$4 \
               AND output.status='done' AND output.recall_eligible \
           ) \
         ORDER BY summary.level DESC,summary.created_at DESC LIMIT $5",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(summary_type)
    .bind(limit.max(1))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    decode_summaries(rows)
}

pub async fn list_latest_thread_summaries_at_level_for_project(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_type: &str,
    level: i64,
    project_subject_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    let mut q = build_summary_query(
        "SELECT data FROM engine_summaries",
        thread_id,
        Some(tenant_id),
        Some(source_id),
        Some(summary_type),
        Some("done"),
        Some(level.max(0)),
    );
    q.push(" AND project_partition_key=")
        .push_bind(project_subject_id.unwrap_or("__none__"));
    q.push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(limit.max(1));
    fetch(db, q).await
}
pub async fn list_thread_summaries(
    db: &Db,
    values: ListSummariesQuery<'_>,
) -> Result<(Vec<EngineSummary>, bool), String> {
    let mut q = build_summary_query(
        "SELECT data FROM engine_summaries",
        values.thread_id,
        values.tenant_id,
        values.source_id,
        values.summary_type,
        values.status,
        values.level,
    );
    if let Some(project_id) = values
        .project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let project_subject_id = if project_id.starts_with("project:") {
            project_id.to_string()
        } else {
            format!("project:{project_id}")
        };
        q.push(" AND BTRIM(COALESCE(data #>> '{metadata,project_subject_id}',''))=")
            .push_bind(project_subject_id);
    }
    if values.project_id_missing {
        q.push(" AND BTRIM(COALESCE(data #>> '{metadata,project_subject_id}',''))=''");
    }
    if let Some(cursor) = values.cursor()? {
        q.push(" AND (-level,created_at,id)>(-")
            .push_bind(cursor.level)
            .push(",")
            .push_bind(cursor.created_at)
            .push(",")
            .push_bind(cursor.id)
            .push(")");
    }
    let limit = values.limit.clamp(1, 500);
    q.push(" ORDER BY -level ASC,created_at ASC,id ASC LIMIT ")
        .push_bind(limit + 1)
        .push(" OFFSET ")
        .push_bind(values.offset.max(0));
    let mut items = fetch(db, q).await?;
    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    Ok((items, has_more))
}
async fn fetch(
    db: &Db,
    mut q: sqlx::QueryBuilder<sqlx::Postgres>,
) -> Result<Vec<EngineSummary>, String> {
    decode_summaries(
        q.build_query_scalar::<Json<serde_json::Value>>()
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?,
    )
}
