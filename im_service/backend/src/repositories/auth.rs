use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::ClientOptions;
use mongodb::options::FindOptions;
use mongodb::Client;
use sha2::{Digest, Sha256};
use std::env;

use crate::db::Db;
use crate::models::{CreateImUserRequest, ImUser, UpdateImUserRequest};

use super::{normalize_optional_text, now_rfc3339};

pub const ADMIN_USER_ID: &str = "admin";
pub const ADMIN_ROLE: &str = "admin";
pub const USER_ROLE: &str = "user";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct MemoryAuthUser {
    pub user_id: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

fn collection(db: &Db) -> mongodb::Collection<ImUser> {
    db.collection::<ImUser>("users")
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn sync_users_from_memory(db: &Db, mongodb_uri: &str) -> Result<usize, String> {
    let source_database = env::var("MEMORY_SERVER_MONGODB_DATABASE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "memory_server".to_string());

    let mut options = ClientOptions::parse(mongodb_uri)
        .await
        .map_err(|e| format!("invalid mongodb uri for memory user sync: {e}"))?;
    options.app_name = Some("im_service_user_sync".to_string());
    let client = Client::with_options(options).map_err(|e| e.to_string())?;
    let source_db = client.database(source_database.as_str());
    let source_collection = source_db.collection::<MemoryAuthUser>("auth_users");
    let cursor = source_collection
        .find(doc! {})
        .await
        .map_err(|e| format!("load memory auth users failed: {e}"))?;
    let source_users: Vec<MemoryAuthUser> = cursor
        .try_collect()
        .await
        .map_err(|e| format!("collect memory auth users failed: {e}"))?;

    for user in source_users.iter() {
        let username = user.user_id.trim();
        if username.is_empty() {
            continue;
        }

        collection(db)
            .update_one(
                doc! {"username": username},
                doc! {
                    "$set": {
                        "id": username,
                        "username": username,
                        "password_hash": &user.password_hash,
                        "role": user.role.trim(),
                        "status": "active",
                        "updated_at": user.updated_at.trim(),
                    },
                    "$setOnInsert": {
                        "display_name": username,
                        "avatar_url": Bson::Null,
                        "created_at": user.created_at.trim(),
                    }
                },
            )
            .upsert(true)
            .await
            .map_err(|e| format!("sync im user {} failed: {}", username, e))?;
    }

    Ok(source_users.len())
}

pub async fn ensure_default_admin(db: &Db) -> Result<(), String> {
    let now = now_rfc3339();
    collection(db)
        .update_one(
            doc! {"username": ADMIN_USER_ID},
            doc! {
                "$set": {
                    "id": ADMIN_USER_ID,
                    "username": ADMIN_USER_ID,
                    "role": ADMIN_ROLE,
                    "status": "active",
                    "updated_at": &now,
                },
                "$setOnInsert": {
                    "display_name": "Administrator",
                    "avatar_url": mongodb::bson::Bson::Null,
                    "password_hash": hash_password("admin"),
                    "created_at": &now,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_user_by_username(db: &Db, username: &str) -> Result<Option<ImUser>, String> {
    collection(db)
        .find_one(doc! {"username": username.trim()})
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_user(db: &Db, req: CreateImUserRequest) -> Result<ImUser, String> {
    let username = req.username.trim().to_string();
    let password = req.password.trim().to_string();
    if username.is_empty() || password.is_empty() {
        return Err("username/password required".to_string());
    }

    let now = now_rfc3339();
    let user = ImUser {
        id: username.clone(),
        username: username.clone(),
        display_name: req
            .display_name
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| username.clone()),
        avatar_url: normalize_optional_text(req.avatar_url.as_deref()),
        password_hash: hash_password(password.as_str()),
        role: req
            .role
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| USER_ROLE.to_string()),
        status: "active".to_string(),
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
    username: &str,
    req: UpdateImUserRequest,
) -> Result<Option<ImUser>, String> {
    let Some(existing) = get_user_by_username(db, username).await? else {
        return Ok(None);
    };

    let mut update_fields = doc! {
        "updated_at": now_rfc3339(),
    };

    if let Some(display_name) = normalize_optional_text(req.display_name.as_deref()) {
        update_fields.insert("display_name", display_name);
    }
    if let Some(avatar_url) = normalize_optional_text(req.avatar_url.as_deref()) {
        update_fields.insert("avatar_url", avatar_url);
    }
    if let Some(password) = normalize_optional_text(req.password.as_deref()) {
        update_fields.insert("password_hash", hash_password(password.as_str()));
    }
    if let Some(role) = normalize_optional_text(req.role.as_deref()) {
        update_fields.insert("role", role);
    }
    if let Some(status) = normalize_optional_text(req.status.as_deref()) {
        update_fields.insert("status", status);
    }

    collection(db)
        .update_one(doc! {"id": &existing.id}, doc! {"$set": update_fields})
        .await
        .map_err(|e| e.to_string())?;

    get_user_by_username(db, username).await
}

pub async fn verify_user_password(
    db: &Db,
    username: &str,
    password: &str,
) -> Result<Option<ImUser>, String> {
    let user = get_user_by_username(db, username).await?;
    let Some(user) = user else {
        return Ok(None);
    };

    if user.password_hash == hash_password(password.trim()) {
        Ok(Some(user))
    } else {
        Ok(None)
    }
}

pub async fn list_users(db: &Db, limit: i64) -> Result<Vec<ImUser>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"created_at": 1, "username": 1})
        .limit(Some(limit.max(1)))
        .build();

    let cursor = collection(db)
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}
