// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::common::decode_summaries;
use crate::db::Db;
use crate::models::EngineSummary;
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

pub async fn list_pending_summary_partition_keys_by_level(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    level: i64,
) -> Result<Vec<String>, String> {
    sqlx::query_scalar(
        "SELECT project_partition_key FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND summary_type='thread_incremental' AND level=$4 AND status='done' \
         AND rollup_status='pending' AND recall_eligible AND project_partition_key IS NOT NULL \
         GROUP BY project_partition_key ORDER BY min(created_at),project_partition_key",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(level)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn list_pending_summaries_by_level_and_partition(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    level: i64,
) -> Result<Vec<EngineSummary>, String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND project_partition_key=$4 AND summary_type='thread_incremental' AND level=$5 \
         AND status='done' AND rollup_status='pending' AND recall_eligible ORDER BY created_at,id",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(level)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    decode_summaries(rows)
}

pub async fn list_summaries_by_ids(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_ids: &[String],
) -> Result<Vec<EngineSummary>, String> {
    if summary_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND id=ANY($4) AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(summary_ids)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    let summaries = decode_summaries(rows)?;
    let mut by_id = summaries
        .into_iter()
        .map(|summary| (summary.id.clone(), summary))
        .collect::<std::collections::HashMap<_, _>>();
    summary_ids
        .iter()
        .map(|id| {
            by_id
                .remove(id)
                .ok_or_else(|| format!("frozen rollup summary is missing: {id}"))
        })
        .collect()
}

pub async fn list_summaries_by_ids_in_source(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_ids: &[String],
) -> Result<Vec<EngineSummary>, String> {
    if summary_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=ANY($3) \
         AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_ids)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    let summaries = decode_summaries(rows)?;
    let mut by_id = summaries
        .into_iter()
        .map(|summary| (summary.id.clone(), summary))
        .collect::<std::collections::HashMap<_, _>>();
    summary_ids
        .iter()
        .map(|id| {
            by_id
                .remove(id)
                .ok_or_else(|| format!("frozen subject-memory summary is missing: {id}"))
        })
        .collect()
}
pub async fn list_threads_with_pending_rollups(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_level: i64,
    limit: i64,
) -> Result<Vec<(String, String, String)>, String> {
    let mut q=QueryBuilder::<Postgres>::new("SELECT tenant_id,source_id,thread_id FROM engine_summaries WHERE recall_eligible AND summary_type='thread_incremental' AND status='done' AND rollup_status='pending' AND level<=");
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
