// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sqlx::types::Json;
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct DerivedEventOutbox {
    pub tenant_id: String,
    pub source_id: String,
    pub event_id: String,
    pub event_kind: String,
    pub aggregate_kind: String,
    pub aggregate_id: String,
    pub project_partition_key: Option<String>,
    pub dispatch_version: i64,
    pub generation_id: Option<String>,
    pub payload: Json<Value>,
    pub status: String,
    pub attempt_count: i64,
    pub lease_owner: Option<String>,
    pub lease_token: Option<String>,
}

const RETURNING_EVENT_COLUMNS: &str = "event.tenant_id,event.source_id,event.event_id,event.event_kind,event.aggregate_kind,event.aggregate_id,event.project_partition_key,event.dispatch_version,event.generation_id,event.payload,event.status,event.attempt_count,event.lease_owner,event.lease_token";

pub(crate) async fn enqueue_thread_summary_request(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    dispatch_version: i64,
) -> Result<String, String> {
    let aggregate_id = thread_partition_aggregate_id(thread_id, project_partition_key);
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
    .bind(Json(payload))
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(event_id)
}

pub async fn claim_publish_batch(
    db: &Db,
    lease_owner: &str,
    lease_seconds: i64,
    limit: i64,
) -> Result<Vec<DerivedEventOutbox>, String> {
    claim_publish_batch_for_scope(db, lease_owner, lease_seconds, limit, None, None).await
}

pub async fn claim_publish_batch_for_scope(
    db: &Db,
    lease_owner: &str,
    lease_seconds: i64,
    limit: i64,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
) -> Result<Vec<DerivedEventOutbox>, String> {
    let lease_owner = lease_owner.trim();
    if lease_owner.is_empty() {
        return Err("derived event publisher lease_owner is required".to_string());
    }
    let lease_token = format!("melease_{}", Uuid::new_v4());
    sqlx::query_as::<_, DerivedEventOutbox>(sqlx::AssertSqlSafe(format!(
        "WITH candidates AS ( \
           SELECT tenant_id,source_id,event_id FROM engine_derived_event_outbox \
            WHERE next_attempt_at<=now() AND (status='pending' OR (status='claimed' AND lease_expires_at<=now())) \
              AND ($5::TEXT IS NULL OR tenant_id=$5) AND ($6::TEXT IS NULL OR source_id=$6) \
            ORDER BY created_at,event_id LIMIT $1 FOR UPDATE SKIP LOCKED \
         ) \
         UPDATE engine_derived_event_outbox event SET status='claimed',attempt_count=event.attempt_count+1, \
                lease_owner=$2,lease_token=$3,lease_expires_at=now()+make_interval(secs=>$4) \
           FROM candidates candidate \
          WHERE event.tenant_id=candidate.tenant_id AND event.source_id=candidate.source_id \
            AND event.event_id=candidate.event_id \
         RETURNING {RETURNING_EVENT_COLUMNS}"
    )))
    .bind(limit.clamp(1, 10_000))
    .bind(lease_owner)
    .bind(lease_token)
    .bind(lease_seconds.clamp(1, 86_400) as f64)
    .bind(tenant_id)
    .bind(source_id)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn mark_published(db: &Db, event: &DerivedEventOutbox) -> Result<bool, String> {
    let lease_token = required_lease_token(event)?;
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let published = sqlx::query(
        "UPDATE engine_derived_event_outbox SET status='published',published_at=now(), \
         lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,last_error_code=NULL,last_error_detail=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND event_id=$3 AND status='claimed' AND lease_token=$4",
    )
    .bind(&event.tenant_id)
    .bind(&event.source_id)
    .bind(&event.event_id)
    .bind(lease_token)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected()
        > 0;
    if !published {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(false);
    }
    match event.event_kind.as_str() {
        "thread_summary_requested" => update_partition_published(&mut tx, event).await?,
        value => {
            return Err(format!(
                "unsupported derived event kind for publisher: {value}"
            ))
        }
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(true)
}

pub async fn mark_publish_failed(
    db: &Db,
    event: &DerivedEventOutbox,
    error_code: &str,
    max_attempts: i64,
    retry_delay_seconds: i64,
) -> Result<Option<String>, String> {
    let lease_token = required_lease_token(event)?;
    let error_code = error_code.trim();
    if error_code.is_empty() {
        return Err("derived event error_code is required".to_string());
    }
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let status = sqlx::query_scalar::<_, String>(
        "UPDATE engine_derived_event_outbox SET \
         status=CASE WHEN attempt_count>=$5 THEN 'dead_letter' ELSE 'pending' END, \
         next_attempt_at=CASE WHEN attempt_count>=$5 THEN next_attempt_at \
           ELSE now()+make_interval(secs=>$6) END, \
         dead_lettered_at=CASE WHEN attempt_count>=$5 THEN now() ELSE dead_lettered_at END, \
         last_error_code=$7,last_error_detail=NULL,lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND event_id=$3 AND status='claimed' AND lease_token=$4 \
         RETURNING status",
    )
    .bind(&event.tenant_id)
    .bind(&event.source_id)
    .bind(&event.event_id)
    .bind(lease_token)
    .bind(max_attempts.max(1))
    .bind(retry_delay_seconds.clamp(0, 86_400) as f64)
    .bind(error_code)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some(status) = status else {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(None);
    };
    if event.event_kind == "thread_summary_requested" {
        update_partition_failed(&mut tx, event, error_code, status.as_str()).await?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(Some(status))
}

#[allow(clippy::too_many_arguments)]
pub async fn consume_thread_summary_request(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    dispatch_version: i64,
    worker_version: &str,
) -> Result<bool, String> {
    if !matches!(worker_version, "v1" | "v2") {
        return Err("partition consumer writer version must be v1 or v2".to_string());
    }
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let state = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT authoritative_writer,summary_dispatch_version,summary_dispatch_consumed_version \
         FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND project_partition_key=$4 FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "thread partition not found for derived event consume".to_string())?;
    if state.0 != worker_version {
        return Err(format!(
            "partition consumer {worker_version} is not authoritative; expected {}",
            state.0
        ));
    }
    if dispatch_version > state.1 {
        return Err(
            "derived event dispatch version is ahead of durable partition state".to_string(),
        );
    }
    if dispatch_version <= state.2 {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(false);
    }
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         summary_dispatch_consumed_version=GREATEST(summary_dispatch_consumed_version,$5), \
         summary_dispatch_consumed_at=now(),summary_dispatch_last_error=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(dispatch_version)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(true)
}

pub async fn rearm_dead_lettered_thread_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    dead_letter_version: i64,
) -> Result<Option<String>, String> {
    let aggregate_id = thread_partition_aggregate_id(thread_id, project_partition_key);
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let dead_letter_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM engine_derived_event_outbox \
         WHERE tenant_id=$1 AND source_id=$2 AND event_kind='thread_summary_requested' \
         AND aggregate_kind='thread_partition' AND aggregate_id=$3 AND dispatch_version=$4 \
         AND status='dead_letter')",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(&aggregate_id)
    .bind(dead_letter_version)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if !dead_letter_exists {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(None);
    }
    let next_version = sqlx::query_scalar::<_, i64>(
        "UPDATE engine_thread_summary_partitions SET \
         summary_dispatch_version=summary_dispatch_version+1,summary_dispatch_pending=true, \
         summary_dispatch_requested_at=now(),summary_dispatch_last_error=NULL, \
         summary_dispatch_dead_letter_version=NULL,summary_dispatch_dead_lettered_at=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND summary_dispatch_version=$5 AND summary_dispatch_dead_letter_version=$5 \
         RETURNING summary_dispatch_version",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(dead_letter_version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some(next_version) = next_version else {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(None);
    };
    let event_id = enqueue_thread_summary_request(
        &mut tx,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
        next_version,
    )
    .await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(Some(event_id))
}

fn required_lease_token(event: &DerivedEventOutbox) -> Result<&str, String> {
    event
        .lease_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "derived event lease_token is required for CAS update".to_string())
}

fn thread_partition_aggregate_id(thread_id: &str, project_partition_key: &str) -> String {
    format!("{thread_id}\u{1f}{project_partition_key}")
}

async fn update_partition_published(
    tx: &mut Transaction<'_, Postgres>,
    event: &DerivedEventOutbox,
) -> Result<(), String> {
    let partition = event
        .project_partition_key
        .as_deref()
        .ok_or_else(|| "thread summary event is missing project_partition_key".to_string())?;
    let thread_id = event
        .payload
        .0
        .get("thread_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "thread summary event payload is missing thread_id".to_string())?;
    let result = sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         summary_dispatch_published_version=GREATEST(summary_dispatch_published_version,$5), \
         summary_dispatch_published_at=now(), \
         summary_dispatch_pending=CASE WHEN summary_dispatch_version<=$5 THEN false ELSE true END, \
         summary_dispatch_last_error=NULL,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
         AND summary_dispatch_version>=$5",
    )
    .bind(&event.tenant_id)
    .bind(&event.source_id)
    .bind(thread_id)
    .bind(partition)
    .bind(event.dispatch_version)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() != 1 {
        return Err("thread partition is missing or behind the published event".to_string());
    }
    let compatibility = sqlx::query(
        "UPDATE engine_threads SET \
         summary_dispatch_pending=CASE WHEN summary_dispatch_consumed_version<summary_dispatch_version THEN true ELSE summary_dispatch_pending END, \
         summary_dispatch_requested_at=COALESCE(summary_dispatch_requested_at,now()) \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_dispatch_version>=$4",
    )
    .bind(&event.tenant_id)
    .bind(&event.source_id)
    .bind(thread_id)
    .bind(event.dispatch_version)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if compatibility.rows_affected() != 1 {
        return Err(
            "compatibility summary dispatch is missing or behind the published event".to_string(),
        );
    }
    Ok(())
}

async fn update_partition_failed(
    tx: &mut Transaction<'_, Postgres>,
    event: &DerivedEventOutbox,
    error_code: &str,
    event_status: &str,
) -> Result<(), String> {
    let partition = event
        .project_partition_key
        .as_deref()
        .ok_or_else(|| "thread summary event is missing project_partition_key".to_string())?;
    let thread_id = event
        .payload
        .0
        .get("thread_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "thread summary event payload is missing thread_id".to_string())?;
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET \
         summary_dispatch_last_error=$6,summary_dispatch_last_failed_at=now(), \
         summary_dispatch_dead_letter_version=CASE WHEN $7='dead_letter' AND summary_dispatch_version=$5 THEN $5 ELSE summary_dispatch_dead_letter_version END, \
         summary_dispatch_dead_lettered_at=CASE WHEN $7='dead_letter' AND summary_dispatch_version=$5 THEN now() ELSE summary_dispatch_dead_lettered_at END, \
         summary_dispatch_pending=CASE WHEN $7='dead_letter' AND summary_dispatch_version=$5 THEN false ELSE summary_dispatch_pending END, \
         updated_at=now() WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4",
    )
    .bind(&event.tenant_id)
    .bind(&event.source_id)
    .bind(thread_id)
    .bind(partition)
    .bind(event.dispatch_version)
    .bind(error_code)
    .bind(event_status)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}
