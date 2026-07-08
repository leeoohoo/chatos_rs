// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};

use crate::repositories::auth_users::AuthUserRecord;

const DEFAULT_LEGACY_AUTH_MONGODB_URI: &str = "mongodb://admin:admin@127.0.0.1:27018/admin";
const DEFAULT_LEGACY_AUTH_MONGODB_DATABASE: &str = "legacy_auth";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyAuthUserRecord {
    pub user_id: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

impl LegacyAuthUserRecord {
    pub fn into_auth_user_record(self) -> AuthUserRecord {
        AuthUserRecord {
            user_id: self.user_id,
            password_hash: self.password_hash,
            role: self.role,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub async fn list_users() -> Result<Vec<LegacyAuthUserRecord>, String> {
    let client = mongodb::Client::with_uri_str(legacy_mongodb_uri().as_str())
        .await
        .map_err(|e| format!("connect legacy auth store failed: {e}"))?;
    let collection = client
        .database(legacy_mongodb_database().as_str())
        .collection::<LegacyAuthUserRecord>("auth_users");

    let mut cursor = collection
        .find(None, None)
        .await
        .map_err(|e| format!("list legacy auth users failed: {e}"))?;

    let mut items = Vec::new();
    while let Some(item) = cursor
        .try_next()
        .await
        .map_err(|e| format!("iterate legacy auth users failed: {e}"))?
    {
        if !item.user_id.trim().is_empty() {
            items.push(item);
        }
    }
    Ok(items)
}

fn legacy_mongodb_uri() -> String {
    std::env::var("LEGACY_AUTH_MONGODB_URI")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_LEGACY_AUTH_MONGODB_URI.to_string())
}

fn legacy_mongodb_database() -> String {
    std::env::var("LEGACY_AUTH_MONGODB_DATABASE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_LEGACY_AUTH_MONGODB_DATABASE.to_string())
}
