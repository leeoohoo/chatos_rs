// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::doc;

use crate::db::Db;
use crate::models::EngineSource;

use super::common::{hash_secret, normalize_optional_text_ref, source_collection, tenant_bson};
use super::writes::is_retired_source_id;

pub async fn list_sources(
    db: &Db,
    tenant_id: Option<&str>,
    source_type: Option<&str>,
    status: Option<&str>,
    sdk_enabled: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSource>, String> {
    let mut filter = doc! {};
    if tenant_id.is_some() {
        filter.insert("tenant_id", tenant_bson(tenant_id));
    }
    if let Some(value) = normalize_optional_text_ref(source_type) {
        filter.insert("source_type", value);
    }
    if let Some(value) = normalize_optional_text_ref(status) {
        filter.insert("status", value);
    }
    if let Some(value) = sdk_enabled {
        filter.insert("sdk_enabled", value);
    }

    let cursor = source_collection(db)
        .find(filter)
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(10_000))
        .await
        .map_err(|err| err.to_string())?;

    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn count_sources(db: &Db) -> Result<i64, String> {
    source_collection(db)
        .count_documents(doc! {})
        .await
        .map(|count| count as i64)
        .map_err(|err| err.to_string())
}

pub async fn verify_source_secret(
    db: &Db,
    source_id: &str,
    secret_key: &str,
) -> Result<Option<EngineSource>, String> {
    let normalized_source_id = source_id.trim();
    let normalized_secret = secret_key.trim();
    if normalized_source_id.is_empty() || normalized_secret.is_empty() {
        return Ok(None);
    }
    if is_retired_source_id(normalized_source_id) {
        return Ok(None);
    }

    let hashed_secret = hash_secret(normalized_secret);
    let cursor = source_collection(db)
        .find(doc! {
            "source_id": normalized_source_id,
            "status": "active",
            "sdk_enabled": true,
            "secret_key_hash": hashed_secret,
        })
        .limit(2)
        .await
        .map_err(|err| err.to_string())?;
    let matches: Vec<EngineSource> = cursor.try_collect().await.map_err(|err| err.to_string())?;

    if matches.len() > 1 {
        return Err(format!(
            "source_id {} is not unique across active sdk-enabled tenants",
            normalized_source_id
        ));
    }

    Ok(matches.into_iter().next())
}

pub async fn is_source_active(db: &Db, source_id: &str) -> Result<bool, String> {
    let normalized_source_id = source_id.trim();
    if normalized_source_id.is_empty() || is_retired_source_id(normalized_source_id) {
        return Ok(false);
    }
    let source = source_collection(db)
        .find_one(doc! {
            "source_id": normalized_source_id,
            "status": "active",
        })
        .await
        .map_err(|err| err.to_string())?;
    Ok(source.is_some())
}
