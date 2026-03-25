use mongodb::bson::{doc, Bson};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{SyncTurnRuntimeSnapshotRequest, TurnRuntimeSnapshot};

use super::{normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<TurnRuntimeSnapshot> {
    db.collection::<TurnRuntimeSnapshot>("turn_runtime_snapshots")
}

pub async fn get_turn_runtime_snapshot(
    db: &Db,
    session_id: &str,
    turn_id: &str,
) -> Result<Option<TurnRuntimeSnapshot>, String> {
    collection(db)
        .find_one(doc! {"session_id": session_id, "turn_id": turn_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_turn_runtime_snapshot(
    db: &Db,
    session_id: &str,
    turn_id: &str,
    user_id: &str,
    req: SyncTurnRuntimeSnapshotRequest,
) -> Result<TurnRuntimeSnapshot, String> {
    let now = now_rfc3339();
    let status = normalize_status(req.status.as_deref());
    let snapshot_source = normalize_snapshot_source(req.snapshot_source.as_deref());
    let snapshot_version = req.snapshot_version.unwrap_or(1).max(1);
    let captured_at = req.captured_at.unwrap_or_else(|| now.clone());

    let mut set_doc = doc! {
        "session_id": session_id,
        "turn_id": turn_id,
        "user_id": user_id,
        "status": status,
        "snapshot_source": snapshot_source,
        "snapshot_version": snapshot_version,
        "updated_at": now.clone(),
    };

    if req.user_message_id.is_some() {
        let user_message_id = normalize_optional_text(req.user_message_id.as_deref())
            .map(Bson::String)
            .unwrap_or(Bson::Null);
        set_doc.insert("user_message_id", user_message_id);
    }

    if let Some(system_messages) = req.system_messages {
        let value = mongodb::bson::to_bson(&system_messages).map_err(|e| e.to_string())?;
        set_doc.insert("system_messages", value);
    }

    if let Some(tools) = req.tools {
        let value = mongodb::bson::to_bson(&tools).map_err(|e| e.to_string())?;
        set_doc.insert("tools", value);
    }

    if req.runtime.is_some() {
        let runtime = req
            .runtime
            .and_then(|value| mongodb::bson::to_bson(&value).ok())
            .unwrap_or(Bson::Null);
        set_doc.insert("runtime", runtime);
    }

    collection(db)
        .update_one(
            doc! {"session_id": session_id, "turn_id": turn_id},
            doc! {
                "$set": set_doc,
                "$setOnInsert": {
                    "id": Uuid::new_v4().to_string(),
                    "captured_at": captured_at,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    get_turn_runtime_snapshot(db, session_id, turn_id)
        .await?
        .ok_or_else(|| "upserted turn runtime snapshot not found".to_string())
}

fn normalize_status(status: Option<&str>) -> String {
    let raw = normalize_optional_text(status).unwrap_or_else(|| "unknown".to_string());
    match raw.to_ascii_lowercase().as_str() {
        "running" => "running".to_string(),
        "completed" => "completed".to_string(),
        "failed" => "failed".to_string(),
        _ => "unknown".to_string(),
    }
}

fn normalize_snapshot_source(source: Option<&str>) -> String {
    let raw = normalize_optional_text(source).unwrap_or_else(|| "captured".to_string());
    match raw.to_ascii_lowercase().as_str() {
        "captured" => "captured".to_string(),
        "missing" => "missing".to_string(),
        _ => "captured".to_string(),
    }
}
