use mongodb::bson::{doc, Bson, Document};

use crate::models::user::{User, UserRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_from_doc(doc: &Document) -> Option<User> {
    let id = doc.get_str("id").ok()?.to_string();
    let email = doc.get_str("email").ok()?.to_string();
    let password_hash = doc.get_str("password_hash").ok()?.to_string();
    let display_name = doc.get_str("display_name").ok().map(|v| v.to_string());
    let status = doc
        .get_str("status")
        .ok()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "active".to_string());
    let last_login_at = doc.get_str("last_login_at").ok().map(|v| v.to_string());
    let created_at = doc.get_str("created_at").ok().unwrap_or("").to_string();
    let updated_at = doc.get_str("updated_at").ok().unwrap_or("").to_string();
    Some(User {
        id,
        email,
        password_hash,
        display_name,
        status,
        last_login_at,
        created_at,
        updated_at,
    })
}

pub async fn create_user(user: &User) -> Result<(), String> {
    let user_mongo = user.clone();
    let user_sqlite = user.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(user_mongo.id.clone())),
                ("email", Bson::String(user_mongo.email.clone())),
                (
                    "password_hash",
                    Bson::String(user_mongo.password_hash.clone()),
                ),
                (
                    "display_name",
                    crate::core::values::optional_string_bson(user_mongo.display_name.clone()),
                ),
                ("status", Bson::String(user_mongo.status.clone())),
                (
                    "last_login_at",
                    crate::core::values::optional_string_bson(user_mongo.last_login_at.clone()),
                ),
                ("created_at", Bson::String(user_mongo.created_at.clone())),
                ("updated_at", Bson::String(user_mongo.updated_at.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("users")
                    .insert_one(doc, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO users (id, email, password_hash, display_name, status, last_login_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&user_sqlite.id)
                    .bind(&user_sqlite.email)
                    .bind(&user_sqlite.password_hash)
                    .bind(&user_sqlite.display_name)
                    .bind(&user_sqlite.status)
                    .bind(&user_sqlite.last_login_at)
                    .bind(&user_sqlite.created_at)
                    .bind(&user_sqlite.updated_at)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn get_user_by_email(email: &str) -> Result<Option<User>, String> {
    with_db(
        |db| {
            let email = email.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("users")
                    .find_one(doc! { "email": &email }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_from_doc(&d)))
            })
        },
        |pool| {
            let email = email.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE email = ?")
                    .bind(&email)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_user()))
            })
        },
    )
    .await
}

pub async fn get_user_by_id(id: &str) -> Result<Option<User>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("users")
                    .find_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_from_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_user()))
            })
        },
    )
    .await
}

pub async fn update_last_login_at(id: &str) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("users")
                    .update_one(
                        doc! { "id": &id },
                        doc! { "$set": { "last_login_at": now_mongo.clone(), "updated_at": now_mongo.clone() } },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE users SET last_login_at = ?, updated_at = ? WHERE id = ?")
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}
