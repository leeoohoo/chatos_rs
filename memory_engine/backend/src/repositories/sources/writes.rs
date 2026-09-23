// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, StoredEngineSource, UpsertSourceRequest};
use crate::repositories::postgres::{decode, json, timestamp};

use super::common::{normalize_optional_text, RETIRED_SOURCE_IDS};

pub fn is_retired_source_id(source_id: &str) -> bool {
    RETIRED_SOURCE_IDS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(source_id.trim()))
}

pub async fn upsert_source(
    db: &Db,
    source_id: &str,
    req: UpsertSourceRequest,
) -> Result<StoredEngineSource, String> {
    let source_id = source_id.trim();
    if source_id.is_empty() {
        return Err("source_id is required".to_string());
    }
    if is_retired_source_id(source_id) {
        return Err(format!("source_id {source_id} is retired"));
    }

    let existing = sqlx::query_as::<_, (Json<serde_json::Value>, Option<String>)>(
        "SELECT data,secret_key_hash FROM engine_sources WHERE source_id=$1",
    )
    .bind(source_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?;
    let now = now_rfc3339();
    let (id, created_at, secret_key_hint, key_last_rotated_at, secret_key_hash) =
        if let Some((data, secret_key_hash)) = existing {
            let source: StoredEngineSource = decode(data)?;
            (
                source.id,
                source.created_at,
                source.secret_key_hint,
                source.key_last_rotated_at,
                secret_key_hash,
            )
        } else {
            (
                format!("src_{}", Uuid::new_v4()),
                now.clone(),
                None,
                None,
                None,
            )
        };
    let source = StoredEngineSource {
        id,
        tenant_id: normalize_optional_text(req.tenant_id),
        source_id: source_id.to_string(),
        source_type: req.source_type,
        name: req.name,
        description: req.description,
        config: req.config,
        status: req.status.unwrap_or_else(|| "active".to_string()),
        sdk_enabled: req.sdk_enabled.unwrap_or(true),
        secret_key_hint,
        key_last_rotated_at,
        secret_key_hash: secret_key_hash.clone(),
        created_at,
        updated_at: now,
    };
    let stored = json(&source)?;
    sqlx::query(
        "INSERT INTO engine_sources \
         (id,tenant_id,source_id,source_type,status,sdk_enabled,secret_key_hash,created_at,updated_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
         ON CONFLICT(source_id) DO UPDATE SET tenant_id=EXCLUDED.tenant_id, \
         source_type=EXCLUDED.source_type,status=EXCLUDED.status,sdk_enabled=EXCLUDED.sdk_enabled, \
         updated_at=EXCLUDED.updated_at,data=EXCLUDED.data",
    )
    .bind(&source.id)
    .bind(&source.tenant_id)
    .bind(&source.source_id)
    .bind(&source.source_type)
    .bind(&source.status)
    .bind(source.sdk_enabled)
    .bind(secret_key_hash)
    .bind(timestamp(&source.created_at)?)
    .bind(timestamp(&source.updated_at)?)
    .bind(stored)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_sources WHERE source_id=$1",
    )
    .bind(source_id)
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())
    .and_then(decode)
}
