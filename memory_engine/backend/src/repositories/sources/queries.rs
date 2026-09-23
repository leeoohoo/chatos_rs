// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

use crate::db::Db;
use crate::models::StoredEngineSource;
use crate::repositories::postgres::decode;

use super::common::{hash_secret, normalize_optional_text_ref};
use super::writes::is_retired_source_id;

pub async fn list_sources(
    db: &Db,
    tenant_id: Option<&str>,
    source_type: Option<&str>,
    status: Option<&str>,
    sdk_enabled: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<Vec<StoredEngineSource>, String> {
    let mut query =
        QueryBuilder::<Postgres>::new("SELECT data,secret_key_hash FROM engine_sources WHERE TRUE");
    if let Some(value) = normalize_optional_text_ref(tenant_id) {
        query.push(" AND tenant_id=").push_bind(value);
    }
    if let Some(value) = normalize_optional_text_ref(source_type) {
        query.push(" AND source_type=").push_bind(value);
    }
    if let Some(value) = normalize_optional_text_ref(status) {
        query.push(" AND status=").push_bind(value);
    }
    if let Some(value) = sdk_enabled {
        query.push(" AND sdk_enabled=").push_bind(value);
    }
    query
        .push(" ORDER BY updated_at DESC,created_at DESC LIMIT ")
        .push_bind(limit.clamp(1, 10_000))
        .push(" OFFSET ")
        .push_bind(offset.max(0));
    query
        .build_query_as::<(Json<serde_json::Value>, Option<String>)>()
        .fetch_all(db)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(decode_source)
        .collect()
}

pub async fn count_sources(db: &Db) -> Result<i64, String> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engine_sources")
        .fetch_one(db)
        .await
        .map_err(|error| error.to_string())
}

pub async fn verify_source_secret(
    db: &Db,
    source_id: &str,
    secret_key: &str,
) -> Result<Option<StoredEngineSource>, String> {
    let source_id = source_id.trim();
    let secret_key = secret_key.trim();
    if source_id.is_empty() || secret_key.is_empty() || is_retired_source_id(source_id) {
        return Ok(None);
    }
    sqlx::query_as::<_, (Json<serde_json::Value>, Option<String>)>(
        "SELECT data,secret_key_hash FROM engine_sources WHERE source_id=$1 AND status='active' \
         AND sdk_enabled AND secret_key_hash=$2",
    )
    .bind(source_id)
    .bind(hash_secret(secret_key))
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode_source)
    .transpose()
}

pub async fn is_source_active(db: &Db, source_id: &str) -> Result<bool, String> {
    let source_id = source_id.trim();
    if source_id.is_empty() || is_retired_source_id(source_id) {
        return Ok(false);
    }
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM engine_sources WHERE source_id=$1 AND status='active')",
    )
    .bind(source_id)
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())
}

fn decode_source(
    (data, secret_key_hash): (Json<serde_json::Value>, Option<String>),
) -> Result<StoredEngineSource, String> {
    let mut source: StoredEngineSource = decode(data)?;
    source.secret_key_hash = secret_key_hash;
    Ok(source)
}
