// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

use crate::models::EngineRecord;
use crate::repositories::postgres::decode;

pub(crate) fn build_record_query<'a>(
    select: &str,
    thread_id: &'a str,
    tenant_id: Option<&'a str>,
    source_id: Option<&'a str>,
    role: Option<&'a str>,
    record_type: Option<&'a str>,
    summary_status: Option<&'a str>,
) -> QueryBuilder<'a, Postgres> {
    let mut query = QueryBuilder::new(select);
    query.push(" WHERE thread_id=").push_bind(thread_id);
    append(&mut query, "tenant_id", tenant_id);
    append(&mut query, "source_id", source_id);
    append(&mut query, "role", role);
    append(&mut query, "record_type", record_type);
    if let Some(value) = normalized(summary_status) {
        if value == "pending" {
            query.push(" AND (summary_status='pending' OR summary_status='')");
        } else {
            query.push(" AND summary_status=").push_bind(value);
        }
    }
    query
}

fn append<'a>(query: &mut QueryBuilder<'a, Postgres>, column: &str, value: Option<&'a str>) {
    if let Some(value) = normalized(value) {
        query.push(" AND ").push(column).push("=").push_bind(value);
    }
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(crate) fn decode_records(
    rows: Vec<Json<serde_json::Value>>,
) -> Result<Vec<EngineRecord>, String> {
    rows.into_iter().map(decode).collect()
}

pub(crate) fn summary_status_is_pending(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), None | Some("") | Some("pending"))
}

pub(crate) fn estimate_record_summary_tokens(
    created_at: &str,
    role: &str,
    content: &str,
    structured_payload: Option<&Value>,
    metadata: Option<&Value>,
) -> i64 {
    let mut parts = vec![format!("[{}][{}]", created_at, role)];
    if !content.trim().is_empty() {
        parts.push(content.to_string());
    }
    if let Some(metadata) = metadata {
        if let Some(reasoning) = metadata
            .get("reasoning")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            parts.push(format!("[reasoning]\n{}", reasoning));
        }
        if let Some(tool_calls) = metadata
            .get("tool_calls")
            .or_else(|| metadata.get("toolCalls"))
            .filter(|v| !v.is_null())
        {
            parts.push(format!("[tool_calls]\n{}", tool_calls));
        }
        if let Some(result) = metadata.get("structured_result").filter(|v| !v.is_null()) {
            parts.push(format!("[tool_result]\n{}", result));
        }
    }
    if let Some(payload) = structured_payload.filter(|v| !v.is_null()) {
        parts.push(format!("[structured_payload]\n{}", payload));
    }
    estimate_tokens_text(&parts.join("\n"))
}

pub(crate) fn estimate_pending_record_tokens(record: &EngineRecord) -> i64 {
    estimate_record_summary_tokens(
        &record.created_at,
        &record.role,
        &record.content,
        record.structured_payload.as_ref(),
        record.metadata.as_ref(),
    )
}

fn estimate_tokens_text(text: &str) -> i64 {
    let chars = text.chars().count() as i64;
    let bytes = text.len() as i64;
    ((chars * 3 + bytes.saturating_sub(chars) * 4 + 11) / 12).max(1)
}
