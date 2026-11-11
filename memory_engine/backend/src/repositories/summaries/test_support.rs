// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::EngineSummary;
use crate::repositories::postgres::{json, timestamp};

use super::common::new_summary;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_test_summary(
    db: &Db,
    id: Option<&str>,
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
    project_partition_key: Option<&str>,
) -> Result<EngineSummary, String> {
    let mut summary = new_summary(
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        summary_type,
        level,
        source_digest,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        metadata,
    );
    if let Some(id) = id {
        summary.id = id.to_string();
    }
    let partition = project_partition_key.unwrap_or("__none__");
    sqlx::query(
        "INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type, \
         level,source_digest,status,rollup_status,subject_memory_summarized, \
         rollup_dispatch_pending,rollup_dispatch_version,rollup_dispatch_requested_at, \
         subject_memory_source_dispatch_pending,subject_memory_source_dispatch_version, \
         subject_memory_source_dispatch_requested_at,created_at,updated_at,data, \
         project_partition_key,recall_eligible) \
         SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,true,1,$12,true,1,$12,$12,$13,$14,$15,true \
         WHERE EXISTS(SELECT 1 FROM engine_threads WHERE tenant_id=$2 AND source_id=$3 AND id=$4)",
    )
    .bind(&summary.id)
    .bind(&summary.tenant_id)
    .bind(&summary.source_id)
    .bind(&summary.thread_id)
    .bind(&summary.subject_id)
    .bind(&summary.summary_type)
    .bind(summary.level)
    .bind(&summary.source_digest)
    .bind(&summary.status)
    .bind(&summary.rollup_status)
    .bind(summary.subject_memory_summarized)
    .bind(timestamp(&summary.created_at)?)
    .bind(timestamp(&summary.updated_at)?)
    .bind(json(&summary)?)
    .bind(partition)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_test_thread_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    project_subject_id: Option<&str>,
) -> Result<EngineSummary, String> {
    insert_test_summary(
        db,
        None,
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        "thread_incremental",
        0,
        None,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        Some(serde_json::json!({"generator":"test"})),
        project_subject_id,
    )
    .await
}
