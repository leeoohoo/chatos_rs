// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use sqlx::types::Json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::Db;

const MAX_ATTEMPTS: i64 = 8;

#[derive(Debug, Clone, FromRow)]
pub struct PartitionRebuildClaim {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub project_partition_key: String,
    pub rebuild_kind: String,
    pub migration_run_id: String,
    pub reason: String,
    pub attempt_count: i64,
    pub attempt_number: i64,
    pub lease_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerationEvidence {
    pub generation_ids: Vec<String>,
    pub config_snapshots: Value,
}

impl PartitionRebuildClaim {
    pub fn lease_token(&self) -> Result<&str, String> {
        self.lease_token
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "partition rebuild claim has no lease token".to_string())
    }
}

pub async fn claim_next(
    db: &Db,
    lease_owner: &str,
    lease_seconds: i64,
) -> Result<Option<PartitionRebuildClaim>, String> {
    let lease_owner = lease_owner.trim();
    if lease_owner.is_empty() {
        return Err("partition rebuild lease_owner is required".to_string());
    }
    let lease_token = format!("rebuild_{}", Uuid::new_v4());
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let claim = sqlx::query_as::<_, PartitionRebuildClaim>(
        "WITH candidate AS ( \
           SELECT b.tenant_id,b.source_id,b.thread_id,b.project_partition_key,b.rebuild_kind, \
                  COALESCE((SELECT MAX(attempt.attempt_number) \
                    FROM engine_partition_rebuild_attempts attempt \
                   WHERE attempt.tenant_id=b.tenant_id AND attempt.source_id=b.source_id \
                     AND attempt.thread_id=b.thread_id \
                     AND attempt.project_partition_key=b.project_partition_key \
                     AND attempt.rebuild_kind=b.rebuild_kind),0)+1 AS attempt_number \
             FROM engine_partition_rebuild_backlog b \
            WHERE (b.status='pending' OR (b.status='running' AND b.lease_expires_at<=now())) \
              AND b.attempt_count<$4 \
              AND NOT EXISTS ( \
                  SELECT 1 FROM engine_partition_rebuild_backlog dependency \
                   WHERE dependency.tenant_id=b.tenant_id AND dependency.source_id=b.source_id \
                     AND dependency.thread_id=b.thread_id \
                     AND dependency.project_partition_key=b.project_partition_key \
                     AND dependency.status<>'completed' \
                     AND ((b.rebuild_kind='thread_rollup' AND dependency.rebuild_kind='thread_l0') \
                       OR (b.rebuild_kind IN ('project_memory','agent_memory') \
                           AND dependency.rebuild_kind='thread_rollup')) \
              ) \
            ORDER BY CASE b.rebuild_kind WHEN 'thread_l0' THEN 0 WHEN 'thread_rollup' THEN 1 \
                         WHEN 'project_memory' THEN 2 ELSE 3 END,b.updated_at,b.thread_id \
            LIMIT 1 FOR UPDATE SKIP LOCKED \
         ) \
         UPDATE engine_partition_rebuild_backlog b SET status='running', \
                attempt_count=b.attempt_count+CASE WHEN b.status='running' THEN 1 ELSE 0 END, \
                lease_owner=$1,lease_token=$2, \
                lease_expires_at=now()+make_interval(secs=>$3),last_error=NULL,updated_at=now() \
           FROM candidate c WHERE b.tenant_id=c.tenant_id AND b.source_id=c.source_id \
             AND b.thread_id=c.thread_id AND b.project_partition_key=c.project_partition_key \
             AND b.rebuild_kind=c.rebuild_kind \
         RETURNING b.tenant_id,b.source_id,b.thread_id,b.project_partition_key,b.rebuild_kind, \
                   b.migration_run_id,b.reason,b.attempt_count+1 AS attempt_count, \
                   c.attempt_number,b.lease_token",
    )
    .bind(lease_owner)
    .bind(&lease_token)
    .bind(lease_seconds.clamp(30, 86_400) as f64)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if let Some(claim) = &claim {
        sqlx::query(
            "UPDATE engine_partition_rebuild_attempts SET status='failed', \
             error_detail=$6,finished_at=now() WHERE tenant_id=$1 AND source_id=$2 \
             AND thread_id=$3 AND project_partition_key=$4 AND rebuild_kind=$5 \
             AND status='running'",
        )
        .bind(&claim.tenant_id)
        .bind(&claim.source_id)
        .bind(&claim.thread_id)
        .bind(&claim.project_partition_key)
        .bind(&claim.rebuild_kind)
        .bind(Json(json!({"code": "rebuild_lease_expired"})))
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            "INSERT INTO engine_partition_rebuild_attempts( \
             id,tenant_id,source_id,thread_id,project_partition_key,rebuild_kind,migration_run_id, \
             attempt_number,status,lease_owner,lease_token,cursor_before) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,'running',$9,$10,$11)",
        )
        .bind(format!("rbat_{}", Uuid::new_v4()))
        .bind(&claim.tenant_id)
        .bind(&claim.source_id)
        .bind(&claim.thread_id)
        .bind(&claim.project_partition_key)
        .bind(&claim.rebuild_kind)
        .bind(&claim.migration_run_id)
        .bind(claim.attempt_number)
        .bind(lease_owner)
        .bind(&lease_token)
        .bind(Json(json!({
            "rebuild_kind": claim.rebuild_kind,
            "attempt_number": claim.attempt_number,
            "retry_count": claim.attempt_count,
            "reason": claim.reason,
        })))
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(claim)
}

pub async fn record_waiting(
    db: &Db,
    claim: &PartitionRebuildClaim,
    cursor: Value,
) -> Result<bool, String> {
    finish_attempt(db, claim, "waiting", "pending", cursor, None).await
}

pub async fn record_completed(
    db: &Db,
    claim: &PartitionRebuildClaim,
    cursor: Value,
) -> Result<bool, String> {
    let evidence = generation_evidence(db, claim).await?;
    let token = claim.lease_token()?;
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let completed = sqlx::query(
        "UPDATE engine_partition_rebuild_backlog SET status='completed',lease_owner=NULL, \
         lease_token=NULL,lease_expires_at=NULL,last_error=NULL,updated_at=now(),completed_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
           AND rebuild_kind=$5 AND status='running' AND lease_token=$6",
    )
    .bind(&claim.tenant_id)
    .bind(&claim.source_id)
    .bind(&claim.thread_id)
    .bind(&claim.project_partition_key)
    .bind(&claim.rebuild_kind)
    .bind(token)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected()
        == 1;
    if !completed {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(false);
    }
    update_attempt(&mut tx, claim, "completed", cursor.clone(), &evidence, None).await?;
    sqlx::query(
        "INSERT INTO engine_reliability_migration_actions( \
         migration_run_id,tenant_id,source_id,action_kind,object_kind,object_id, \
         project_partition_key,before_state,after_state) \
         VALUES($1,$2,$3,'complete_partition_rebuild','thread_partition',$4,$5,$6,$7) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&claim.migration_run_id)
    .bind(&claim.tenant_id)
    .bind(&claim.source_id)
    .bind(format!(
        "{}:{}:{}",
        claim.thread_id, claim.project_partition_key, claim.rebuild_kind
    ))
    .bind(&claim.project_partition_key)
    .bind(Json(json!({
        "status": "running",
        "attempt_number": claim.attempt_number,
        "retry_count": claim.attempt_count,
    })))
    .bind(Json(json!({
        "status": "completed",
        "cursor": cursor,
        "generation_ids": evidence.generation_ids,
    })))
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let (total, done, failed) = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT COUNT(*),COUNT(*) FILTER (WHERE status='completed'), \
                COUNT(*) FILTER (WHERE status='failed') \
           FROM engine_partition_rebuild_backlog WHERE migration_run_id=$1",
    )
    .bind(&claim.migration_run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let phase = if done == total && total > 0 {
        "m3b_complete"
    } else {
        "rebuilding"
    };
    sqlx::query(
        "UPDATE engine_reliability_migration_runs SET cursor=$2,statistics=statistics || $3, \
         status=CASE WHEN $4 THEN 'completed' ELSE status END, \
         finished_at=CASE WHEN $4 THEN now() ELSE finished_at END WHERE id=$1 AND status='running'",
    )
    .bind(&claim.migration_run_id)
    .bind(Json(json!({
        "phase": phase,
        "last_thread_id": claim.thread_id,
        "last_project_partition_key": claim.project_partition_key,
        "last_rebuild_kind": claim.rebuild_kind,
        "completed_items": done,
    })))
    .bind(Json(json!({
        "backlog_items": total,
        "completed_items": done,
        "failed_items": failed,
    })))
    .bind(done == total && total > 0)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(true)
}

pub async fn record_failed(
    db: &Db,
    claim: &PartitionRebuildClaim,
    error_code: &str,
) -> Result<bool, String> {
    let next_status = if claim.attempt_count >= MAX_ATTEMPTS {
        "failed"
    } else {
        "pending"
    };
    finish_attempt(
        db,
        claim,
        "failed",
        next_status,
        json!({"phase": "failed", "error_code": error_code}),
        Some(error_code),
    )
    .await
}

async fn finish_attempt(
    db: &Db,
    claim: &PartitionRebuildClaim,
    attempt_status: &str,
    backlog_status: &str,
    cursor: Value,
    error_code: Option<&str>,
) -> Result<bool, String> {
    let evidence = generation_evidence(db, claim).await?;
    let token = claim.lease_token()?;
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let updated = sqlx::query(
        "UPDATE engine_partition_rebuild_backlog SET status=$7,lease_owner=NULL,lease_token=NULL, \
         lease_expires_at=NULL,last_error=$8, \
         attempt_count=CASE WHEN $9::BIGINT IS NULL THEN attempt_count ELSE $9 END,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
           AND rebuild_kind=$5 AND status='running' AND lease_token=$6",
    )
    .bind(&claim.tenant_id)
    .bind(&claim.source_id)
    .bind(&claim.thread_id)
    .bind(&claim.project_partition_key)
    .bind(&claim.rebuild_kind)
    .bind(token)
    .bind(backlog_status)
    .bind(error_code.map(|value| Json(json!({"code": value}))))
    .bind(error_code.map(|_| claim.attempt_count))
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected()
        == 1;
    if !updated {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(false);
    }
    update_attempt(
        &mut tx,
        claim,
        attempt_status,
        cursor,
        &evidence,
        error_code,
    )
    .await?;
    if backlog_status == "failed" {
        sqlx::query(
            "UPDATE engine_reliability_migration_runs SET status='failed',error_detail=$2, \
             cursor=$3,finished_at=now() WHERE id=$1 AND status='running'",
        )
        .bind(&claim.migration_run_id)
        .bind(Json(json!({
            "error_code": error_code,
            "thread_id": claim.thread_id,
            "project_partition_key": claim.project_partition_key,
            "rebuild_kind": claim.rebuild_kind,
        })))
        .bind(Json(json!({"phase": "m3b_failed"})))
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(true)
}

async fn update_attempt(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    claim: &PartitionRebuildClaim,
    status: &str,
    cursor: Value,
    evidence: &GenerationEvidence,
    error_code: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE engine_partition_rebuild_attempts SET status=$7,cursor_after=$8, \
         generation_ids=$9,config_snapshots=$10,error_detail=$11,finished_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
           AND rebuild_kind=$5 AND attempt_number=$6",
    )
    .bind(&claim.tenant_id)
    .bind(&claim.source_id)
    .bind(&claim.thread_id)
    .bind(&claim.project_partition_key)
    .bind(&claim.rebuild_kind)
    .bind(claim.attempt_number)
    .bind(status)
    .bind(Json(cursor))
    .bind(&evidence.generation_ids)
    .bind(Json(&evidence.config_snapshots))
    .bind(error_code.map(|value| Json(json!({"code": value}))))
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn generation_evidence(
    db: &Db,
    claim: &PartitionRebuildClaim,
) -> Result<GenerationEvidence, String> {
    let rows = sqlx::query_as::<_, (String, Json<Value>)>(
        "SELECT DISTINCT generation.generation_id,generation.config_snapshot \
           FROM engine_generation_commits generation \
           LEFT JOIN engine_summaries summary \
             ON generation.output_kind='thread_summary' \
            AND summary.tenant_id=generation.tenant_id AND summary.source_id=generation.source_id \
            AND summary.id=generation.output_id \
           LEFT JOIN engine_memory_derivation_edges edge \
             ON generation.output_kind='subject_memory' \
            AND edge.tenant_id=generation.tenant_id AND edge.source_id=generation.source_id \
            AND edge.generation_id=generation.generation_id AND edge.status='active' \
           LEFT JOIN engine_summaries input_summary \
             ON edge.input_kind='thread_summary' \
            AND input_summary.tenant_id=edge.tenant_id AND input_summary.source_id=edge.source_id \
            AND input_summary.id=edge.input_id \
          WHERE generation.tenant_id=$1 AND generation.source_id=$2 AND generation.status='committed' \
            AND ((summary.thread_id=$3 AND summary.project_partition_key=$4) \
              OR (input_summary.thread_id=$3 AND edge.input_project_partition_key=$4)) \
          ORDER BY generation.generation_id",
    )
    .bind(&claim.tenant_id)
    .bind(&claim.source_id)
    .bind(&claim.thread_id)
    .bind(&claim.project_partition_key)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(GenerationEvidence {
        generation_ids: rows.iter().map(|row| row.0.clone()).collect(),
        config_snapshots: Value::Array(rows.into_iter().map(|row| row.1 .0).collect()),
    })
}
