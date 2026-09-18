// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::common::decode_summaries;
use crate::db::Db;
use crate::models::EngineSummary;
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

#[allow(clippy::too_many_arguments)]
pub async fn list_summaries_by_thread_label(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_label: &str,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    subject_memory_summarized: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSummary>, String> {
    list_internal(
        db,
        tenant_id,
        source_id,
        thread_label,
        summary_type,
        status,
        level,
        subject_memory_summarized,
        None,
        limit,
        offset,
    )
    .await
}
pub async fn list_summaries_by_thread_label_for_subject_memory_scope(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_label: &str,
    summary_type: &str,
    scope_key: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    list_internal(
        db,
        tenant_id,
        source_id,
        thread_label,
        Some(summary_type),
        Some("done"),
        None,
        None,
        Some(scope_key),
        limit,
        0,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn list_internal(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    label: &str,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    subject_memory_summarized: Option<i64>,
    scope_key: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSummary>, String> {
    let label = label.trim();
    if label.is_empty() {
        return Ok(Vec::new());
    }
    let mut q=QueryBuilder::<Postgres>::new("SELECT s.data FROM engine_summaries s JOIN engine_threads t ON t.id=s.thread_id WHERE s.tenant_id=");
    q.push_bind(tenant_id)
        .push(" AND s.source_id=")
        .push_bind(source_id)
        .push(" AND t.data->'labels' ? ")
        .push_bind(label);
    append_prefixed(&mut q, "s.summary_type", summary_type);
    append_prefixed(&mut q, "s.status", status);
    if let Some(v) = level {
        q.push(" AND s.level=").push_bind(v);
    }
    if let Some(v) = subject_memory_summarized {
        q.push(" AND s.subject_memory_summarized=")
            .push_bind(v.max(0));
    }
    if let Some(v) = scope_key.map(str::trim).filter(|v| !v.is_empty()) {
        q.push(" AND NOT (")
            .push_bind(v)
            .push("=ANY(s.subject_memory_scope_keys))");
    }
    q.push(" ORDER BY s.created_at,s.level DESC LIMIT ")
        .push_bind(limit.clamp(1, 5000))
        .push(" OFFSET ")
        .push_bind(offset.max(0));
    decode_summaries(
        q.build_query_scalar::<Json<serde_json::Value>>()
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?,
    )
}
fn append_prefixed<'a>(q: &mut QueryBuilder<'a, Postgres>, column: &str, value: Option<&'a str>) {
    if let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) {
        q.push(" AND ").push(column).push("=").push_bind(v);
    }
}
