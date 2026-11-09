// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::{FromRow, Postgres, Transaction};

use crate::db::Db;

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct PartitionSummaryClaim {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub project_partition_key: String,
    pub migration_state: String,
    pub authoritative_writer: String,
    pub summary_job_run_id: String,
    pub pending_record_count: i64,
    pub pending_summary_tokens: i64,
}

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
    let state = sqlx::query_as::<_, (String, String)>(
        "SELECT migration_state,authoritative_writer FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some((migration_state, authoritative_writer)) = state else {
        return Err("thread partition could not be created in the requested scope".to_string());
    };
    let writer_state_is_valid = matches!(
        (migration_state.as_str(), authoritative_writer.as_str()),
        ("backfilling" | "ready", "v1") | ("authoritative", "v2")
    );
    if !writer_state_is_valid {
        return Err("thread partition writer state is inconsistent".to_string());
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

#[allow(clippy::too_many_arguments)]
pub async fn try_claim_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    worker_version: &str,
    job_run_id: &str,
    lease_seconds: i64,
) -> Result<Option<PartitionSummaryClaim>, String> {
    if !matches!(worker_version, "v1" | "v2") {
        return Err("partition summary worker version must be v1 or v2".to_string());
    }
    if job_run_id.trim().is_empty() {
        return Err("partition summary job_run_id is required".to_string());
    }
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let state = sqlx::query_as::<_, (String, String)>(
        "SELECT migration_state,authoritative_writer FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "thread partition not found for summary claim".to_string())?;
    if state.1 != worker_version {
        return Err(format!(
            "partition summary worker {worker_version} is not authoritative; expected {}",
            state.1
        ));
    }
    let claim = sqlx::query_as::<_, PartitionSummaryClaim>(
        "UPDATE engine_thread_summary_partitions SET summary_status='running',summary_job_run_id=$5, \
         summary_locked_at=now(),summary_lock_expires_at=now()+make_interval(secs=>$6),updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND pending_record_count>0 AND (summary_status<>'running' OR summary_lock_expires_at IS NULL \
           OR summary_lock_expires_at<=now()) \
         RETURNING tenant_id,source_id,thread_id,project_partition_key,migration_state,authoritative_writer, \
                   summary_job_run_id,pending_record_count,pending_summary_tokens",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(job_run_id)
    .bind(lease_seconds.clamp(1, 86_400) as f64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(claim)
}

#[allow(clippy::too_many_arguments)]
pub async fn release_summary_claim(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    worker_version: &str,
    job_run_id: &str,
    processed_record_count: i64,
    processed_tokens: i64,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         pending_record_count=GREATEST(pending_record_count-$7,0), \
         pending_summary_tokens=GREATEST(pending_summary_tokens-$8,0), \
         summary_status=CASE WHEN GREATEST(pending_record_count-$7,0)>0 THEN 'pending' ELSE 'idle' END, \
         summary_job_run_id=NULL,summary_locked_at=NULL,summary_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND authoritative_writer=$5 AND summary_status='running' AND summary_job_run_id=$6",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(worker_version)
    .bind(job_run_id)
    .bind(processed_record_count.max(0))
    .bind(processed_tokens.max(0))
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

pub async fn mark_ready(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    expected_pending_record_count: i64,
    expected_pending_summary_tokens: i64,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET migration_state='ready',updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND migration_state='backfilling' AND authoritative_writer='v1' \
         AND pending_record_count=$5 AND pending_summary_tokens=$6",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(expected_pending_record_count.max(0))
    .bind(expected_pending_summary_tokens.max(0))
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

pub async fn cutover_to_v2(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET migration_state='authoritative', \
         authoritative_writer='v2',updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND migration_state='ready' AND authoritative_writer='v1' AND summary_status<>'running'",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}
