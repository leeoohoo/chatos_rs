// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::{now_rfc3339, StoredEngineSource, StoredRotateSourceSecretResponse};
use crate::repositories::postgres::{decode, json, timestamp};

use super::common::{
    build_secret_key_hint, generate_secret_key, hash_secret, normalize_optional_text_ref,
};

pub async fn rotate_source_secret(
    db: &Db,
    source_id: &str,
    tenant_id: Option<&str>,
) -> Result<Option<StoredRotateSourceSecretResponse>, String> {
    let source_id = source_id.trim();
    if source_id.is_empty() {
        return Err("source_id is required".to_string());
    }
    let tenant_id = normalize_optional_text_ref(tenant_id);
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let data = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_sources WHERE source_id=$1 \
         AND tenant_id IS NOT DISTINCT FROM $2 FOR UPDATE",
    )
    .bind(source_id)
    .bind(&tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some(data) = data else {
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(None);
    };

    let secret_key = generate_secret_key();
    let mut source: StoredEngineSource = decode(data)?;
    let now = now_rfc3339();
    source.sdk_enabled = true;
    source.secret_key_hint = Some(build_secret_key_hint(&secret_key));
    source.key_last_rotated_at = Some(now.clone());
    source.updated_at = now;
    sqlx::query(
        "UPDATE engine_sources SET sdk_enabled=true,secret_key_hash=$2,updated_at=$3,data=$4 \
         WHERE source_id=$1 AND tenant_id IS NOT DISTINCT FROM $5",
    )
    .bind(source_id)
    .bind(hash_secret(&secret_key))
    .bind(timestamp(&source.updated_at)?)
    .bind(json(&source)?)
    .bind(&tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(Some(StoredRotateSourceSecretResponse {
        source,
        secret_key,
    }))
}
