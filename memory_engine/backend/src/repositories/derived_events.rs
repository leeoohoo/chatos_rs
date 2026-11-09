// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(crate) async fn enqueue_thread_summary_request(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    dispatch_version: i64,
) -> Result<String, String> {
    let aggregate_id = format!("{thread_id}\u{1f}{project_partition_key}");
    let event_id = format!("mevt_{}", Uuid::new_v4());
    let payload = serde_json::json!({
        "schema_version": "memory_derived_event_v1",
        "thread_id": thread_id,
        "project_partition_key": project_partition_key,
        "dispatch_version": dispatch_version,
    });
    sqlx::query(
        "INSERT INTO engine_derived_event_outbox(tenant_id,source_id,event_id,event_kind,aggregate_kind,aggregate_id,project_partition_key,dispatch_version,payload) \
         VALUES($1,$2,$3,'thread_summary_requested','thread_partition',$4,$5,$6,$7) \
         ON CONFLICT(tenant_id,source_id,event_kind,aggregate_kind,aggregate_id,dispatch_version) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&event_id)
    .bind(aggregate_id)
    .bind(project_partition_key)
    .bind(dispatch_version)
    .bind(sqlx::types::Json(payload))
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(event_id)
}
