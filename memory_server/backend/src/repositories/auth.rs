use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::now_rfc3339;

pub const ADMIN_USER_ID: &str = "admin";
pub const ADMIN_ROLE: &str = "admin";
pub const USER_ROLE: &str = "user";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct AuthUser {
    pub user_id: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn ensure_default_admin(pool: &SqlitePool) -> Result<(), String> {
    let now = now_rfc3339();
    let password_hash = hash_password("admin");

    sqlx::query(
        "INSERT INTO auth_users (user_id, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT(user_id) DO NOTHING",
    )
    .bind(ADMIN_USER_ID)
    .bind(password_hash)
    .bind(ADMIN_ROLE)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn get_user_by_id(pool: &SqlitePool, user_id: &str) -> Result<Option<AuthUser>, String> {
    sqlx::query_as::<_, AuthUser>("SELECT * FROM auth_users WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_user(
    pool: &SqlitePool,
    user_id: &str,
    password: &str,
    role: &str,
) -> Result<AuthUser, String> {
    let now = now_rfc3339();
    let password_hash = hash_password(password);

    sqlx::query(
        "INSERT INTO auth_users (user_id, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(password_hash)
    .bind(role)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_user_by_id(pool, user_id)
        .await?
        .ok_or_else(|| "created auth user not found".to_string())
}

pub async fn verify_user_password(
    pool: &SqlitePool,
    user_id: &str,
    password: &str,
) -> Result<Option<AuthUser>, String> {
    let user = get_user_by_id(pool, user_id).await?;
    let Some(user) = user else {
        return Ok(None);
    };

    if user.password_hash == hash_password(password) {
        Ok(Some(user))
    } else {
        Ok(None)
    }
}
