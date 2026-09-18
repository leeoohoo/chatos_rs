// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::common::decode_summaries;
use crate::db::Db;
use crate::models::EngineSummary;
use crate::repositories::postgres::decode;
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

pub async fn find_summary_by_source_digest(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    level: i64,
    source_digest: &str,
) -> Result<Option<EngineSummary>, String> {
    let digest = source_digest.trim();
    if digest.is_empty() {
        return Ok(None);
    }
    sqlx::query_scalar::<_,Json<serde_json::Value>>("SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND level=$4 AND source_digest=$5 LIMIT 1")
        .bind(tenant_id).bind(source_id).bind(thread_id).bind(level).bind(digest).fetch_optional(db).await.map_err(|e|e.to_string())?.map(decode).transpose()
}
pub async fn list_pending_summaries_by_level(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    level: i64,
) -> Result<Vec<EngineSummary>, String> {
    let rows=sqlx::query_scalar::<_,Json<serde_json::Value>>("SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND summary_type='thread_incremental' AND level=$4 AND status='done' AND rollup_status='pending' ORDER BY created_at")
        .bind(tenant_id).bind(source_id).bind(thread_id).bind(level).fetch_all(db).await.map_err(|e|e.to_string())?;
    decode_summaries(rows)
}
pub async fn list_threads_with_pending_rollups(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_level: i64,
    limit: i64,
) -> Result<Vec<(String, String, String)>, String> {
    let mut q=QueryBuilder::<Postgres>::new("SELECT tenant_id,source_id,thread_id FROM engine_summaries WHERE summary_type='thread_incremental' AND status='done' AND rollup_status='pending' AND level<=");
    q.push_bind(max_level.max(0));
    if let Some(v) = tenant_id.map(str::trim).filter(|v| !v.is_empty()) {
        q.push(" AND tenant_id=").push_bind(v);
    }
    if let Some(v) = source_id.map(str::trim).filter(|v| !v.is_empty()) {
        q.push(" AND source_id=").push_bind(v);
    }
    q.push(" GROUP BY tenant_id,source_id,thread_id ORDER BY min(created_at) LIMIT ")
        .push_bind(limit.clamp(1, 500));
    q.build_query_as()
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())
}
