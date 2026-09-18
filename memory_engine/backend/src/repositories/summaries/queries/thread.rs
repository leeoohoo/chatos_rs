// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::common::{build_summary_query, decode_summaries};
use crate::db::Db;
use crate::models::EngineSummary;
use sqlx::types::Json;

use super::super::ListSummariesQuery;

pub async fn list_latest_thread_summaries(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    list_latest_thread_summaries_by_type(
        db,
        tenant_id,
        source_id,
        thread_id,
        "thread_incremental",
        limit,
    )
    .await
}
pub async fn list_latest_thread_summaries_at_level(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_type: &str,
    level: i64,
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
    q.push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(limit.max(1));
    fetch(db, q).await
}
pub async fn list_latest_thread_summaries_by_type(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_type: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    let mut q = build_summary_query(
        "SELECT data FROM engine_summaries",
        thread_id,
        Some(tenant_id),
        Some(source_id),
        Some(summary_type),
        Some("done"),
        None,
    );
    q.push(" ORDER BY level DESC,created_at DESC LIMIT ")
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
    mut q: sqlx::QueryBuilder<'_, sqlx::Postgres>,
) -> Result<Vec<EngineSummary>, String> {
    decode_summaries(
        q.build_query_scalar::<Json<serde_json::Value>>()
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?,
    )
}
