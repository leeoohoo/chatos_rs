// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineThreadSnapshot, ThreadSnapshotLookupResponse, UpsertThreadSnapshotRequest,
};
use crate::repositories::postgres::{decode, json, timestamp};

pub async fn upsert_thread_snapshot(
    db: &Db,
    thread_id: &str,
    snapshot_type: &str,
    turn_id: &str,
    req: UpsertThreadSnapshotRequest,
) -> Result<EngineThreadSnapshot, String> {
    let existing = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_thread_snapshots WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND snapshot_type=$4 AND turn_id=$5",
    )
    .bind(&req.tenant_id)
    .bind(&req.source_id)
    .bind(thread_id)
    .bind(snapshot_type)
    .bind(turn_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode::<EngineThreadSnapshot>)
    .transpose()?;
    let now = now_rfc3339();
    let snapshot = EngineThreadSnapshot {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| format!("tsnap_{}", Uuid::new_v4())),
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        snapshot_type: snapshot_type.to_string(),
        user_message_id: req.user_message_id,
        status: req.status.unwrap_or_else(|| "captured".to_string()),
        snapshot_source: req
            .snapshot_source
            .unwrap_or_else(|| "captured".to_string()),
        snapshot_version: req.snapshot_version.unwrap_or(1).max(1),
        payload: req.payload,
        metadata: req.metadata,
        captured_at: req.captured_at.unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    sqlx::query(
        "INSERT INTO engine_thread_snapshots \
         (id,tenant_id,source_id,thread_id,turn_id,snapshot_type,status,snapshot_version, \
          captured_at,updated_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
         ON CONFLICT(tenant_id,source_id,thread_id,turn_id,snapshot_type) DO UPDATE SET \
         status=EXCLUDED.status,snapshot_version=EXCLUDED.snapshot_version, \
         captured_at=EXCLUDED.captured_at,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data",
    )
    .bind(&snapshot.id)
    .bind(&snapshot.tenant_id)
    .bind(&snapshot.source_id)
    .bind(&snapshot.thread_id)
    .bind(&snapshot.turn_id)
    .bind(&snapshot.snapshot_type)
    .bind(&snapshot.status)
    .bind(snapshot.snapshot_version)
    .bind(timestamp(&snapshot.captured_at)?)
    .bind(timestamp(&snapshot.updated_at)?)
    .bind(json(&snapshot)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(snapshot)
}

pub async fn get_latest_thread_snapshot(
    db: &Db,
    thread_id: &str,
    snapshot_type: &str,
    tenant_id: &str,
    source_id: &str,
) -> Result<ThreadSnapshotLookupResponse, String> {
    let snapshot = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_thread_snapshots WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND snapshot_type=$4 \
         ORDER BY captured_at DESC,updated_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(snapshot_type)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode)
    .transpose()?;
    Ok(snapshot_response(thread_id, snapshot_type, None, snapshot))
}

pub async fn get_thread_snapshot_by_turn(
    db: &Db,
    thread_id: &str,
    snapshot_type: &str,
    turn_id: &str,
    tenant_id: &str,
    source_id: &str,
) -> Result<ThreadSnapshotLookupResponse, String> {
    let snapshot = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_thread_snapshots WHERE tenant_id=$1 AND source_id=$2 \
         AND thread_id=$3 AND snapshot_type=$4 AND turn_id=$5",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(snapshot_type)
    .bind(turn_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode)
    .transpose()?;
    Ok(snapshot_response(
        thread_id,
        snapshot_type,
        Some(turn_id),
        snapshot,
    ))
}

fn snapshot_response(
    thread_id: &str,
    snapshot_type: &str,
    requested_turn_id: Option<&str>,
    snapshot: Option<EngineThreadSnapshot>,
) -> ThreadSnapshotLookupResponse {
    ThreadSnapshotLookupResponse {
        thread_id: thread_id.to_string(),
        turn_id: requested_turn_id
            .map(ToOwned::to_owned)
            .or_else(|| snapshot.as_ref().map(|item| item.turn_id.clone())),
        snapshot_type: snapshot_type.to_string(),
        status: snapshot
            .as_ref()
            .map(|item| item.status.clone())
            .unwrap_or_else(|| "missing".to_string()),
        snapshot_source: snapshot
            .as_ref()
            .map(|item| item.snapshot_source.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        snapshot,
    }
}
