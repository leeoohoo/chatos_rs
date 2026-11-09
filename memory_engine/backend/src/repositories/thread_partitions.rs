// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::{Postgres, Transaction};

pub(crate) async fn lock_or_create(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO engine_thread_summary_partitions(tenant_id,source_id,thread_id,project_partition_key,summary_status) \
         SELECT $1,$2,$3,$4,'idle' WHERE EXISTS(SELECT 1 FROM engine_threads \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3) ON CONFLICT DO NOTHING",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    let found = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .is_some();
    if !found {
        return Err("thread partition could not be created in the requested scope".to_string());
    }
    Ok(())
}

pub(crate) async fn apply_ingest_delta(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    pending_record_count_delta: i64,
    pending_summary_tokens_delta: i64,
    force_dispatch: bool,
) -> Result<Option<i64>, String> {
    let version = sqlx::query_scalar::<_, Option<i64>>(
        "UPDATE engine_thread_summary_partitions SET \
         pending_record_count=GREATEST(pending_record_count+$5,0), \
         pending_summary_tokens=GREATEST(pending_summary_tokens+$6,0), \
         summary_status=CASE WHEN summary_status='running' THEN summary_status \
           WHEN GREATEST(pending_record_count+$5,0)>0 THEN 'pending' ELSE 'idle' END, \
         summary_dispatch_version=summary_dispatch_version+CASE WHEN $7 AND GREATEST(pending_record_count+$5,0)>0 THEN 1 ELSE 0 END, \
         summary_dispatch_force_version=CASE WHEN $7 AND GREATEST(pending_record_count+$5,0)>0 THEN summary_dispatch_version+1 ELSE summary_dispatch_force_version END, \
         summary_dispatch_pending=CASE WHEN $7 AND GREATEST(pending_record_count+$5,0)>0 THEN true ELSE summary_dispatch_pending END, \
         summary_dispatch_requested_at=CASE WHEN $7 AND GREATEST(pending_record_count+$5,0)>0 THEN now() ELSE summary_dispatch_requested_at END, \
         summary_dispatch_last_error=CASE WHEN $7 THEN NULL ELSE summary_dispatch_last_error END,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         RETURNING CASE WHEN $7 AND pending_record_count>0 THEN summary_dispatch_version ELSE NULL END",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(pending_record_count_delta)
    .bind(pending_summary_tokens_delta)
    .bind(force_dispatch)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(version)
}
