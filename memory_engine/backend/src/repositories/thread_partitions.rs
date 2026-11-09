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
    pub summary_job_run_id: String,
    pub pending_record_count: i64,
    pub pending_summary_tokens: i64,
}

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct PartitionRollupClaim {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub project_partition_key: String,
    pub rollup_job_run_id: String,
}

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct PartitionSummaryDispatch {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub project_partition_key: String,
    pub pending_record_count: i64,
    pub pending_summary_tokens: i64,
    pub summary_status: String,
    pub summary_dispatch_version: i64,
    pub summary_dispatch_published_version: i64,
    pub summary_dispatch_consumed_version: i64,
    pub summary_dispatch_force_version: i64,
}

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct PendingSummaryPartition {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub project_partition_key: String,
}

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct SummaryPartitionState {
    pub pending_record_count: i64,
    pub pending_summary_tokens: i64,
    pub summary_status: String,
    pub summary_job_run_id: Option<String>,
}

impl PartitionSummaryDispatch {
    pub fn forced(&self) -> bool {
        self.summary_dispatch_force_version > self.summary_dispatch_consumed_version
    }
}

pub async fn get_summary_partition_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<Option<SummaryPartitionState>, String> {
    sqlx::query_as(
        "SELECT pending_record_count,pending_summary_tokens,summary_status,summary_job_run_id \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND project_partition_key=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn list_pending_summary_partitions(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    token_threshold: i64,
    limit: i64,
) -> Result<Vec<PendingSummaryPartition>, String> {
    sqlx::query_as(
        "SELECT tenant_id,source_id,thread_id,project_partition_key \
         FROM engine_thread_summary_partitions WHERE summary_status='pending' \
         AND pending_summary_tokens>=$1 AND ($2::TEXT IS NULL OR tenant_id=$2) \
         AND ($3::TEXT IS NULL OR source_id=$3) ORDER BY updated_at,thread_id,project_partition_key \
         LIMIT $4",
    )
    .bind(token_threshold.max(1))
    .bind(tenant_id)
    .bind(source_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn list_ready_summary_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<PartitionSummaryDispatch>, String> {
    sqlx::query_as(
        "SELECT tenant_id,source_id,thread_id,project_partition_key,pending_record_count, \
         pending_summary_tokens,summary_status,summary_dispatch_version, \
         summary_dispatch_published_version,summary_dispatch_consumed_version, \
         summary_dispatch_force_version FROM engine_thread_summary_partitions \
         WHERE pending_record_count>0 AND summary_dispatch_published_version>summary_dispatch_consumed_version \
         AND summary_dispatch_dead_letter_version IS DISTINCT FROM summary_dispatch_published_version \
         AND (summary_status<>'running' OR summary_lock_expires_at IS NULL OR summary_lock_expires_at<=now()) \
         ORDER BY summary_dispatch_requested_at,updated_at LIMIT $1",
    )
    .bind(limit.clamp(1, 1000))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn consume_summary_dispatch(
    db: &Db,
    dispatch: &PartitionSummaryDispatch,
) -> Result<bool, String> {
    consume_thread_summary_request(
        db,
        &dispatch.tenant_id,
        &dispatch.source_id,
        &dispatch.thread_id,
        &dispatch.project_partition_key,
        dispatch.summary_dispatch_published_version,
    )
    .await
}

async fn consume_thread_summary_request(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    version: i64,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         summary_dispatch_consumed_version=GREATEST(summary_dispatch_consumed_version,$5), \
         summary_dispatch_consumed_at=now(),summary_dispatch_last_error=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND summary_dispatch_version>=$5 AND summary_dispatch_consumed_version<$5",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(version)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

pub async fn consume_current_summary_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<bool, String> {
    let version = sqlx::query_scalar::<_, i64>(
        "SELECT summary_dispatch_published_version FROM engine_thread_summary_partitions \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "thread summary partition dispatch is missing".to_string())?;
    consume_thread_summary_request(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
        version,
    )
    .await
}

pub async fn mark_summary_dispatch_failed(
    db: &Db,
    dispatch: &PartitionSummaryDispatch,
    error_code: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET summary_dispatch_last_error=$5, \
         summary_dispatch_last_failed_at=now(),updated_at=now() WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND project_partition_key=$4",
    )
    .bind(&dispatch.tenant_id)
    .bind(&dispatch.source_id)
    .bind(&dispatch.thread_id)
    .bind(&dispatch.project_partition_key)
    .bind(error_code)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn dead_letter_current_summary_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    error_code: &str,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         summary_dispatch_dead_letter_version=summary_dispatch_published_version, \
         summary_dispatch_dead_lettered_at=now(),summary_dispatch_last_error=$5, \
         summary_dispatch_last_failed_at=now(),updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND summary_dispatch_published_version>summary_dispatch_consumed_version",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(error_code)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

pub async fn release_expired_summary_claims(db: &Db, limit: i64) -> Result<u64, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET summary_status=CASE WHEN pending_record_count>0 \
         THEN 'pending' ELSE 'idle' END,summary_job_run_id=NULL,summary_locked_at=NULL, \
         summary_lock_expires_at=NULL,updated_at=now() WHERE (tenant_id,source_id,thread_id,project_partition_key) \
         IN (SELECT tenant_id,source_id,thread_id,project_partition_key FROM engine_thread_summary_partitions \
         WHERE summary_status='running' AND (summary_lock_expires_at IS NULL OR summary_lock_expires_at<=now()) \
         ORDER BY updated_at LIMIT $1 FOR UPDATE SKIP LOCKED)",
    )
    .bind(limit.clamp(1, 1000))
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected())
}

pub async fn release_summary_claim_by_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<u64, String> {
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET summary_status=CASE WHEN pending_record_count>0 \
         THEN 'pending' ELSE 'idle' END,summary_job_run_id=NULL,summary_locked_at=NULL, \
         summary_lock_expires_at=NULL,updated_at=now() WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND summary_job_run_id=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(job_run_id)
    .execute(db)
    .await
    .map(|result| result.rows_affected())
    .map_err(|error| error.to_string())
}

pub async fn release_rollup_claim_by_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<u64, String> {
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET rollup_status='pending',rollup_job_run_id=NULL, \
         rollup_locked_at=NULL,rollup_lock_expires_at=NULL,updated_at=now() WHERE tenant_id=$1 \
         AND source_id=$2 AND thread_id=$3 AND rollup_job_run_id=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(job_run_id)
    .execute(db)
    .await
    .map(|result| result.rows_affected())
    .map_err(|error| error.to_string())
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
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT true FROM engine_thread_summary_partitions \
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
    let Some(true) = exists else {
        return Err("thread partition could not be created in the requested scope".to_string());
    };
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
         summary_dispatch_version=summary_dispatch_version+CASE WHEN ($5>0 OR $7) AND GREATEST(pending_record_count+$5,0)>0 THEN 1 ELSE 0 END, \
         summary_dispatch_force_version=CASE WHEN $7 AND GREATEST(pending_record_count+$5,0)>0 THEN summary_dispatch_version+1 ELSE summary_dispatch_force_version END, \
         summary_dispatch_pending=CASE WHEN ($5>0 OR $7) AND GREATEST(pending_record_count+$5,0)>0 THEN true ELSE summary_dispatch_pending END, \
         summary_dispatch_requested_at=CASE WHEN ($5>0 OR $7) AND GREATEST(pending_record_count+$5,0)>0 THEN now() ELSE summary_dispatch_requested_at END, \
         summary_dispatch_last_error=CASE WHEN $7 THEN NULL ELSE summary_dispatch_last_error END,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         RETURNING CASE WHEN ($5>0 OR $7) AND pending_record_count>0 THEN summary_dispatch_version ELSE NULL END",
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
    job_run_id: &str,
    lease_seconds: i64,
) -> Result<Option<PartitionSummaryClaim>, String> {
    if job_run_id.trim().is_empty() {
        return Err("partition summary job_run_id is required".to_string());
    }
    let claim = sqlx::query_as::<_, PartitionSummaryClaim>(
        "UPDATE engine_thread_summary_partitions SET summary_status='running',summary_job_run_id=$5, \
         summary_locked_at=now(),summary_lock_expires_at=now()+make_interval(secs=>$6),updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND pending_record_count>0 AND (summary_status<>'running' OR summary_lock_expires_at IS NULL \
           OR summary_lock_expires_at<=now()) \
         RETURNING tenant_id,source_id,thread_id,project_partition_key,summary_job_run_id, \
                   pending_record_count,pending_summary_tokens",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(job_run_id)
    .bind(lease_seconds.clamp(1, 86_400) as f64)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(claim)
}

#[allow(clippy::too_many_arguments)]
pub async fn release_summary_claim(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    job_run_id: &str,
    processed_record_count: i64,
    processed_tokens: i64,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         pending_record_count=GREATEST(pending_record_count-$6,0), \
         pending_summary_tokens=GREATEST(pending_summary_tokens-$7,0), \
         summary_status=CASE WHEN GREATEST(pending_record_count-$6,0)>0 THEN 'pending' ELSE 'idle' END, \
         summary_job_run_id=NULL,summary_locked_at=NULL,summary_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND summary_status='running' AND summary_job_run_id=$5",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(job_run_id)
    .bind(processed_record_count.max(0))
    .bind(processed_tokens.max(0))
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

#[allow(clippy::too_many_arguments)]
pub async fn refresh_summary_claim(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    job_run_id: &str,
    lease_seconds: i64,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         summary_lock_expires_at=now()+make_interval(secs=>$6),updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND summary_status='running' AND summary_job_run_id=$5",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(job_run_id)
    .bind(lease_seconds.clamp(1, 86_400) as f64)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

#[allow(clippy::too_many_arguments)]
pub async fn try_claim_rollup(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    job_run_id: &str,
    lease_seconds: i64,
) -> Result<Option<PartitionRollupClaim>, String> {
    if job_run_id.trim().is_empty() {
        return Err("partition rollup job_run_id is required".to_string());
    }
    let claim = sqlx::query_as::<_, PartitionRollupClaim>(
        "UPDATE engine_thread_summary_partitions SET rollup_status='running',rollup_job_run_id=$5, \
         rollup_locked_at=now(),rollup_lock_expires_at=now()+make_interval(secs=>$6),updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND (rollup_status<>'running' OR rollup_lock_expires_at IS NULL OR rollup_lock_expires_at<=now()) \
         RETURNING tenant_id,source_id,thread_id,project_partition_key,rollup_job_run_id",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(job_run_id)
    .bind(lease_seconds.clamp(1, 86_400) as f64)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(claim)
}

#[allow(clippy::too_many_arguments)]
pub async fn release_rollup_claim(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    job_run_id: &str,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET rollup_status='pending', \
         rollup_job_run_id=NULL,rollup_locked_at=NULL,rollup_lock_expires_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND rollup_status='running' AND rollup_job_run_id=$5",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(job_run_id)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

#[allow(clippy::too_many_arguments)]
pub async fn refresh_rollup_claim(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    job_run_id: &str,
    lease_seconds: i64,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         rollup_lock_expires_at=now()+make_interval(secs=>$6),updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND rollup_status='running' AND rollup_job_run_id=$5",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(job_run_id)
    .bind(lease_seconds.clamp(1, 86_400) as f64)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}
