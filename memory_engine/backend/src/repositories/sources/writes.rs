// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSource, UpsertSourceRequest};

use super::common::{
    normalize_optional_text, source_collection, source_filter, tenant_bson, RETIRED_SOURCE_IDS,
};

pub fn is_retired_source_id(source_id: &str) -> bool {
    RETIRED_SOURCE_IDS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(source_id.trim()))
}

fn is_duplicate_source_id_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("duplicate key")
        && normalized.contains("source_id")
        && normalized.contains("engine_sources")
}

pub async fn upsert_source(
    db: &Db,
    source_id: &str,
    req: UpsertSourceRequest,
) -> Result<EngineSource, String> {
    let normalized_source_id = source_id.trim();
    if normalized_source_id.is_empty() {
        return Err("source_id is required".to_string());
    }
    if is_retired_source_id(normalized_source_id) {
        return Err(format!("source_id {normalized_source_id} is retired"));
    }

    let now = now_rfc3339();
    let id = format!("src_{}", Uuid::new_v4());
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let sdk_enabled = req.sdk_enabled.unwrap_or(true);
    let tenant_id = normalize_optional_text(req.tenant_id.clone());
    let filter = source_filter(tenant_id.as_deref(), normalized_source_id);
    let update = doc! {
        "$set": {
            "tenant_id": tenant_bson(tenant_id.as_deref()),
            "source_id": normalized_source_id,
            "source_type": &req.source_type,
            "name": &req.name,
            "description": mongodb::bson::to_bson(&req.description).unwrap_or(Bson::Null),
            "config": mongodb::bson::to_bson(&req.config).unwrap_or(Bson::Null),
            "status": &status,
            "sdk_enabled": sdk_enabled,
            "updated_at": &now,
        },
        "$setOnInsert": {
            "id": id,
            "secret_key_hint": Bson::Null,
            "key_last_rotated_at": Bson::Null,
            "secret_key_hash": Bson::Null,
            "created_at": &now,
        }
    };

    if let Err(err) = source_collection(db)
        .update_one(filter.clone(), update.clone())
        .upsert(true)
        .await
    {
        let message = err.to_string();
        if !is_duplicate_source_id_error(message.as_str()) {
            return Err(message);
        }

        let legacy_filter = doc! { "source_id": normalized_source_id };
        source_collection(db)
            .update_one(legacy_filter.clone(), update)
            .await
            .map_err(|err| err.to_string())?;

        return source_collection(db)
            .find_one(legacy_filter)
            .await
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "upserted source not found".to_string());
    }

    source_collection(db)
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted source not found".to_string())
}
