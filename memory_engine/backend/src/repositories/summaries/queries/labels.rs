// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;

use crate::db::Db;
use crate::models::EngineSummary;

use super::super::common::{
    collect_summaries, collect_summary_thread_ids, summary_collection, thread_collection,
};

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
    list_summaries_by_thread_label_internal(
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
    list_summaries_by_thread_label_internal(
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

async fn list_summaries_by_thread_label_internal(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_label: &str,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    subject_memory_summarized: Option<i64>,
    subject_memory_scope_key: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSummary>, String> {
    let normalized_label = thread_label.trim();
    if normalized_label.is_empty() {
        return Ok(Vec::new());
    }

    let thread_cursor = thread_collection(db)
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "labels": normalized_label,
        })
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(5_000)
        .await
        .map_err(|err| err.to_string())?;

    let thread_ids = collect_summary_thread_ids(thread_cursor).await?;
    if thread_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "thread_id": {"$in": thread_ids},
    };
    if let Some(value) = summary_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        filter.insert("summary_type", value);
    }
    if let Some(value) = status.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("status", value);
    }
    if let Some(value) = level {
        filter.insert("level", value);
    }
    if let Some(value) = subject_memory_summarized {
        filter.insert("subject_memory_summarized", value.max(0));
    }
    if let Some(value) = subject_memory_scope_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        filter.insert("subject_memory_scope_keys", doc! {"$ne": value});
    }

    let cursor = summary_collection(db)
        .find(filter)
        .sort(doc! {"created_at": 1, "level": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.clamp(1, 5_000))
        .await
        .map_err(|err| err.to_string())?;

    collect_summaries(cursor).await
}
