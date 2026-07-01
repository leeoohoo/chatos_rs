// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Document},
    Cursor,
};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSummary};

pub(crate) fn summary_collection(db: &Db) -> mongodb::Collection<EngineSummary> {
    db.collection::<EngineSummary>("engine_summaries")
}

pub(crate) fn thread_collection(db: &Db) -> mongodb::Collection<crate::models::EngineThread> {
    db.collection::<crate::models::EngineThread>("engine_threads")
}

pub(crate) async fn collect_summaries(
    cursor: Cursor<EngineSummary>,
) -> Result<Vec<EngineSummary>, String> {
    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub(crate) async fn collect_summary_thread_ids(
    cursor: Cursor<crate::models::EngineThread>,
) -> Result<Vec<String>, String> {
    let threads: Vec<crate::models::EngineThread> =
        cursor.try_collect().await.map_err(|err| err.to_string())?;
    Ok(threads.into_iter().map(|thread| thread.id).collect())
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

pub(crate) fn build_thread_summary_filter(
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
) -> Document {
    let mut filter = doc! {
        "thread_id": thread_id,
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }
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
    filter
}
