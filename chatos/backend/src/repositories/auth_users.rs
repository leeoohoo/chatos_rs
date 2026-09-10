// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;
use serde::{Deserialize, Serialize};

use crate::repositories::db::with_db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUserRecord {
    pub user_id: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_user_by_id(user_id: &str) -> Result<Option<AuthUserRecord>, String> {
    let user_id = user_id.trim().to_string();
    if user_id.is_empty() {
        return Ok(None);
    }

    with_db(|db| {
        let user_id = user_id.clone();
        Box::pin(async move {
            db.collection::<AuthUserRecord>("auth_users")
                .find_one(doc! { "user_id": &user_id }, None)
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn upsert_user(user: &AuthUserRecord) -> Result<(), String> {
    let user = user.clone();

    with_db(|db| {
        let user = user.clone();
        Box::pin(async move {
            db.collection::<AuthUserRecord>("auth_users")
                .update_one(
                    doc! { "user_id": &user.user_id },
                    doc! {
                        "$set": {
                            "password_hash": &user.password_hash,
                            "role": &user.role,
                            "created_at": &user.created_at,
                            "updated_at": &user.updated_at,
                        },
                        "$setOnInsert": {
                            "user_id": &user.user_id,
                        }
                    },
                    mongodb::options::UpdateOptions::builder()
                        .upsert(true)
                        .build(),
                )
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    })
    .await
}
