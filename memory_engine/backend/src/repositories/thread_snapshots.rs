// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    now_rfc3339, EngineThreadSnapshot, ThreadSnapshotLookupResponse, UpsertThreadSnapshotRequest,
};

pub async fn upsert_thread_snapshot(
    db: &Db,
    thread_id: &str,
    snapshot_type: &str,
    turn_id: &str,
    req: UpsertThreadSnapshotRequest,
) -> Result<EngineThreadSnapshot, String> {
    let now = now_rfc3339();
    let created_at = req.captured_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = now.clone();
    let filter = doc! {
        "tenant_id": &req.tenant_id,
        "source_id": &req.source_id,
        "thread_id": thread_id,
        "snapshot_type": snapshot_type,
        "turn_id": turn_id,
    };
    let id = format!("tsnap_{}", Uuid::new_v4());
    let status = req.status.unwrap_or_else(|| "captured".to_string());
    let snapshot_source = req
        .snapshot_source
        .unwrap_or_else(|| "captured".to_string());
    let snapshot_version = req.snapshot_version.unwrap_or(1).max(1);

    db.collection::<EngineThreadSnapshot>("engine_thread_snapshots")
        .update_one(
            filter.clone(),
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "thread_id": thread_id,
                    "turn_id": turn_id,
                    "snapshot_type": snapshot_type,
                    "user_message_id": mongodb::bson::to_bson(&req.user_message_id).unwrap_or(Bson::Null),
                    "status": &status,
                    "snapshot_source": &snapshot_source,
                    "snapshot_version": snapshot_version,
                    "payload": mongodb::bson::to_bson(&req.payload).unwrap_or(Bson::Null),
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(Bson::Null),
                    "captured_at": &created_at,
                    "updated_at": &updated_at,
                },
                "$setOnInsert": {
                    "id": id,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    db.collection::<EngineThreadSnapshot>("engine_thread_snapshots")
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted snapshot not found".to_string())
}

pub async fn get_latest_thread_snapshot(
    db: &Db,
    thread_id: &str,
    snapshot_type: &str,
    tenant_id: &str,
    source_id: &str,
) -> Result<ThreadSnapshotLookupResponse, String> {
    let snapshot = db
        .collection::<EngineThreadSnapshot>("engine_thread_snapshots")
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "snapshot_type": snapshot_type,
        })
        .sort(doc! {"captured_at": -1, "updated_at": -1})
        .await
        .map_err(|err| err.to_string())?;

    Ok(ThreadSnapshotLookupResponse {
        thread_id: thread_id.to_string(),
        turn_id: snapshot.as_ref().map(|item| item.turn_id.clone()),
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
    })
}

pub async fn get_thread_snapshot_by_turn(
    db: &Db,
    thread_id: &str,
    snapshot_type: &str,
    turn_id: &str,
    tenant_id: &str,
    source_id: &str,
) -> Result<ThreadSnapshotLookupResponse, String> {
    let snapshot = db
        .collection::<EngineThreadSnapshot>("engine_thread_snapshots")
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "snapshot_type": snapshot_type,
            "turn_id": turn_id,
        })
        .await
        .map_err(|err| err.to_string())?;

    Ok(ThreadSnapshotLookupResponse {
        thread_id: thread_id.to_string(),
        turn_id: Some(turn_id.to_string()),
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
    })
}
