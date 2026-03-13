use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use sha2::{Digest, Sha256};

use crate::db::Db;

use super::now_rfc3339;

pub const ADMIN_USER_ID: &str = "admin";
pub const ADMIN_ROLE: &str = "admin";
pub const USER_ROLE: &str = "user";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

fn collection(db: &Db) -> mongodb::Collection<AuthUser> {
    db.collection::<AuthUser>("auth_users")
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn ensure_default_admin(db: &Db) -> Result<(), String> {
    let now = now_rfc3339();
    let password_hash = hash_password("admin");

    collection(db)
        .update_one(
            doc! {"user_id": ADMIN_USER_ID},
            doc! {
                "$setOnInsert": {
                    "user_id": ADMIN_USER_ID,
                    "password_hash": password_hash,
                    "role": ADMIN_ROLE,
                    "created_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn get_user_by_id(db: &Db, user_id: &str) -> Result<Option<AuthUser>, String> {
    collection(db)
        .find_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_user(
    db: &Db,
    user_id: &str,
    password: &str,
    role: &str,
) -> Result<AuthUser, String> {
    let now = now_rfc3339();
    let user = AuthUser {
        user_id: user_id.to_string(),
        password_hash: hash_password(password),
        role: role.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    collection(db)
        .insert_one(user.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(user)
}

pub async fn update_user(
    db: &Db,
    user_id: &str,
    password: Option<&str>,
    role: Option<&str>,
) -> Result<Option<AuthUser>, String> {
    let Some(existing) = get_user_by_id(db, user_id).await? else {
        return Ok(None);
    };

    let mut update_fields = doc! {
        "updated_at": now_rfc3339(),
    };

    if let Some(password) = password {
        update_fields.insert("password_hash", hash_password(password));
    }
    if let Some(role) = role {
        update_fields.insert("role", role);
    }

    collection(db)
        .update_one(
            doc! {"user_id": &existing.user_id},
            doc! {"$set": update_fields},
        )
        .await
        .map_err(|e| e.to_string())?;

    get_user_by_id(db, user_id).await
}

pub async fn delete_user(db: &Db, user_id: &str) -> Result<bool, String> {
    let result = collection(db)
        .delete_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn verify_user_password(
    db: &Db,
    user_id: &str,
    password: &str,
) -> Result<Option<AuthUser>, String> {
    let user = get_user_by_id(db, user_id).await?;
    let Some(user) = user else {
        return Ok(None);
    };

    if user.password_hash == hash_password(password) {
        Ok(Some(user))
    } else {
        Ok(None)
    }
}

pub async fn list_users(db: &Db, limit: i64) -> Result<Vec<AuthUser>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"created_at": 1, "user_id": 1})
        .limit(Some(limit.max(1)))
        .build();

    let cursor = collection(db)
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}
