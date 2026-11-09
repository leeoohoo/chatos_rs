// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{now_rfc3339, EngineRecord, EngineSummary};
use crate::repositories::postgres::{decode, json, timestamp};
use sqlx::types::Json;
use sqlx::{Postgres, Transaction};

pub async fn delete_thread_summary(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    expected_project_partition_key: &str,
) -> Result<usize, String> {
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let restored = delete_thread_summary_in_tx(
        &mut tx,
        thread_id,
        summary_id,
        tenant_id,
        source_id,
        expected_project_partition_key,
    )
    .await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(restored)
}

pub async fn delete_thread_summary_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    thread_id: &str,
    summary_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    expected_project_partition_key: &str,
) -> Result<usize, String> {
    let tenant = tenant_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "tenant_id is required to delete a summary".to_string())?;
    let source = source_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "source_id is required to delete a summary".to_string())?;
    let selected = sqlx::query_as::<_, (Json<serde_json::Value>, Option<String>)>(
        "SELECT data,project_partition_key FROM engine_summaries WHERE tenant_id=$1 \
         AND source_id=$2 AND thread_id=$3 AND id=$4 AND recall_eligible FOR UPDATE",
    )
    .bind(tenant)
    .bind(source)
    .bind(thread_id)
    .bind(summary_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some((data, project_partition_key)) = selected else {
        return Ok(0);
    };
    let project_partition_key = project_partition_key
        .ok_or_else(|| "summary has no canonical project partition".to_string())?;
    if project_partition_key != expected_project_partition_key {
        return Err("summary project partition does not match verified scope".to_string());
    }
    let mut summary = decode::<EngineSummary>(data)?;
    let covered_rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT record.data FROM engine_summary_record_coverage coverage \
         JOIN engine_records record ON record.tenant_id=coverage.tenant_id \
          AND record.source_id=coverage.source_id AND record.id=coverage.record_id \
         WHERE coverage.tenant_id=$1 AND coverage.source_id=$2 AND coverage.summary_id=$3 \
          AND coverage.status='active' AND record.recall_eligible FOR UPDATE OF record",
    )
    .bind(tenant)
    .bind(source)
    .bind(summary_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    let records = covered_rows
        .into_iter()
        .map(decode::<EngineRecord>)
        .collect::<Result<Vec<_>, _>>()?;
    let pending_tokens = records
        .iter()
        .map(crate::repositories::records::estimate_pending_record_tokens)
        .sum::<i64>();
    let now = now_rfc3339();
    for mut record in records.iter().cloned() {
        record.summary_status = "pending".to_string();
        record.summary_id = None;
        record.summarized_at = None;
        sqlx::query(
            "UPDATE engine_records SET summary_status='pending',summary_id=NULL, \
             summary_job_run_id=NULL,summary_started_at=NULL,data=$4 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND recall_eligible",
        )
        .bind(tenant)
        .bind(source)
        .bind(&record.id)
        .bind(json(&record)?)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    sqlx::query(
        "UPDATE engine_summary_record_coverage SET status='retired',retired_at=$4, \
         retired_reason='summary_retired' WHERE tenant_id=$1 AND source_id=$2 \
         AND summary_id=$3 AND status='active'",
    )
    .bind(tenant)
    .bind(source)
    .bind(summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_memory_derivation_edges SET status='retired',retired_at=$4, \
         retired_reason='summary_retired' WHERE tenant_id=$1 AND source_id=$2 AND status='active' \
         AND ((input_kind='thread_summary' AND input_id=$3) \
           OR (output_kind='thread_summary' AND output_id=$3))",
    )
    .bind(tenant)
    .bind(source)
    .bind(summary_id)
    .bind(timestamp(&now)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    crate::repositories::memory_facts::retire_output(
        tx,
        tenant,
        source,
        "thread_summary",
        summary_id,
    )
    .await?;
    summary.status = "retired".to_string();
    summary.updated_at = now.clone();
    sqlx::query(
        "UPDATE engine_summaries SET status='retired',recall_eligible=false,retired_at=$5, \
         retired_reason='business_delete',updated_at=$5,data=$6 WHERE tenant_id=$1 \
         AND source_id=$2 AND thread_id=$3 AND id=$4 AND recall_eligible",
    )
    .bind(tenant)
    .bind(source)
    .bind(thread_id)
    .bind(summary_id)
    .bind(timestamp(&now)?)
    .bind(json(&summary)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if !records.is_empty() {
        crate::repositories::thread_partitions::lock_or_create(
            tx,
            tenant,
            source,
            thread_id,
            project_partition_key.as_str(),
        )
        .await?;
        let version = crate::repositories::thread_partitions::apply_ingest_delta(
            tx,
            tenant,
            source,
            thread_id,
            project_partition_key.as_str(),
            records.len() as i64,
            pending_tokens,
            true,
        )
        .await?;
        if let Some(version) = version {
            crate::repositories::derived_events::enqueue_thread_summary_request(
                tx,
                tenant,
                source,
                thread_id,
                project_partition_key.as_str(),
                version,
            )
            .await?;
        }
    }
    Ok(records.len())
}
