// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;

use crate::db::Db;
use crate::models::{now_rfc3339, RotateSourceSecretResponse};

use super::common::{
    build_secret_key_hint, generate_secret_key, hash_secret, source_collection, source_filter,
};

pub async fn rotate_source_secret(
    db: &Db,
    source_id: &str,
    tenant_id: Option<&str>,
) -> Result<Option<RotateSourceSecretResponse>, String> {
    let normalized_source_id = source_id.trim();
    if normalized_source_id.is_empty() {
        return Err("source_id is required".to_string());
    }

    let filter = source_filter(tenant_id, normalized_source_id);
    let Some(_) = source_collection(db)
        .find_one(filter.clone())
        .await
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };

    let secret_key = generate_secret_key();
    let secret_key_hash = hash_secret(secret_key.as_str());
    let secret_key_hint = build_secret_key_hint(secret_key.as_str());
    let now = now_rfc3339();

    source_collection(db)
        .update_one(
            filter.clone(),
            doc! {
                "$set": {
                    "sdk_enabled": true,
                    "secret_key_hash": secret_key_hash,
                    "secret_key_hint": secret_key_hint,
                    "key_last_rotated_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    let source = source_collection(db)
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "rotated source not found".to_string())?;

    Ok(Some(RotateSourceSecretResponse { source, secret_key }))
}
