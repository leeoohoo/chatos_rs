// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::models::{now_rfc3339, EngineSummary};
use crate::repositories::postgres::decode;

pub(crate) fn decode_summaries(
    rows: Vec<Json<serde_json::Value>>,
) -> Result<Vec<EngineSummary>, String> {
    rows.into_iter().map(decode).collect()
}

pub(crate) fn new_summary(
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_type: &str,
    level: i64,
    source_digest: Option<String>,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    metadata: Option<serde_json::Value>,
) -> EngineSummary {
    let now = now_rfc3339();
    EngineSummary {
        id: format!("sum_{}", Uuid::new_v4()),
        tenant_id: tenant_id.to_string(),
        source_id: source_id.to_string(),
        thread_id: thread_id.to_string(),
        subject_id: subject_id.to_string(),
        summary_type: summary_type.to_string(),
        level: level.max(0),
        source_digest,
        summary_text: summary_text.to_string(),
        source_record_start_id,
        source_record_end_id,
        source_record_count: source_record_count as i64,
        status: "done".to_string(),
        rollup_status: "pending".to_string(),
        rollup_summary_id: None,
        rolled_up_at: None,
        subject_memory_summarized: 0,
        subject_memory_summarized_at: None,
        metadata,
        created_at: now.clone(),
        updated_at: now,
    }
}

pub(crate) fn build_summary_query<'a>(
    select: &str,
    thread_id: &'a str,
    tenant_id: Option<&'a str>,
    source_id: Option<&'a str>,
    summary_type: Option<&'a str>,
    status: Option<&'a str>,
    level: Option<i64>,
) -> QueryBuilder<'a, Postgres> {
    let mut query = QueryBuilder::new(select);
    query.push(" WHERE thread_id=").push_bind(thread_id);
    append(&mut query, "tenant_id", tenant_id);
    append(&mut query, "source_id", source_id);
    append(&mut query, "summary_type", summary_type);
    append(&mut query, "status", status);
    if let Some(level) = level {
        query.push(" AND level=").push_bind(level);
    }
    query
}

pub(crate) fn append<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    column: &str,
    value: Option<&'a str>,
) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        query.push(" AND ").push(column).push("=").push_bind(value);
    }
}
