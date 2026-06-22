use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;

use crate::repositories::db::with_db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUserRecord {
    pub user_id: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateUserResult {
    Created,
    AlreadyExists,
}

#[derive(Debug, Clone, FromRow)]
struct AuthUserRow {
    user_id: String,
    password_hash: String,
    role: String,
    created_at: String,
    updated_at: String,
}

impl AuthUserRow {
    fn into_record(self) -> AuthUserRecord {
        AuthUserRecord {
            user_id: self.user_id,
            password_hash: self.password_hash,
            role: self.role,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn get_user_by_id(user_id: &str) -> Result<Option<AuthUserRecord>, String> {
    let user_id = user_id.trim().to_string();
    if user_id.is_empty() {
        return Ok(None);
    }

    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                db.collection::<AuthUserRecord>("auth_users")
                    .find_one(doc! { "user_id": &user_id }, None)
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let row = sqlx::query_as::<_, AuthUserRow>(
                    "SELECT user_id, password_hash, role, created_at, updated_at FROM auth_users WHERE user_id = ?",
                )
                .bind(&user_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(AuthUserRow::into_record))
            })
        },
    )
    .await
}

pub async fn verify_user_password(
    user_id: &str,
    password: &str,
) -> Result<Option<AuthUserRecord>, String> {
    let Some(user) = get_user_by_id(user_id).await? else {
        return Ok(None);
    };

    if user.password_hash == hash_password(password) {
        Ok(Some(user))
    } else {
        Ok(None)
    }
}

pub async fn upsert_user(user: &AuthUserRecord) -> Result<(), String> {
    let user = user.clone();

    with_db(
        |db| {
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
        },
        |pool| {
            let user = user.clone();
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO auth_users (user_id, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT(user_id) DO UPDATE SET password_hash = excluded.password_hash, role = excluded.role, created_at = excluded.created_at, updated_at = excluded.updated_at",
                )
                .bind(&user.user_id)
                .bind(&user.password_hash)
                .bind(&user.role)
                .bind(&user.created_at)
                .bind(&user.updated_at)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn create_user(user: &AuthUserRecord) -> Result<CreateUserResult, String> {
    let user = user.clone();

    with_db(
        |db| {
            let user = user.clone();
            Box::pin(async move {
                match db.collection::<AuthUserRecord>("auth_users").insert_one(user, None).await {
                    Ok(_) => Ok(CreateUserResult::Created),
                    Err(err) => {
                        if err.to_string().contains("E11000") {
                            return Ok(CreateUserResult::AlreadyExists);
                        }
                        Err(err.to_string())
                    }
                }
            })
        },
        |pool| {
            let user = user.clone();
            Box::pin(async move {
                match sqlx::query(
                    "INSERT INTO auth_users (user_id, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&user.user_id)
                .bind(&user.password_hash)
                .bind(&user.role)
                .bind(&user.created_at)
                .bind(&user.updated_at)
                .execute(pool)
                .await
                {
                    Ok(_) => Ok(CreateUserResult::Created),
                    Err(err) => {
                        let message = err.to_string();
                        if message.contains("UNIQUE constraint failed") {
                            return Ok(CreateUserResult::AlreadyExists);
                        }
                        Err(message)
                    }
                }
            })
        },
    )
    .await
}
