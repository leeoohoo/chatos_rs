use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use sqlx::Row;

use crate::models::system_context::{SystemContext, SystemContextRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};

fn normalize_doc(doc: &Document) -> Option<SystemContext> {
    Some(SystemContext {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        content: doc.get_str("content").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok()?.to_string(),
        is_active: doc.get_bool("is_active").unwrap_or(false),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn list_system_contexts(user_id: &str) -> Result<Vec<SystemContext>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let mut cursor = db
                    .collection::<Document>("system_contexts")
                    .find(doc! { "user_id": user_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    docs.push(doc);
                }
                let mut items: Vec<SystemContext> =
                    docs.into_iter().filter_map(|d| normalize_doc(&d)).collect();
                items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query_as::<_, SystemContextRow>(
                    "SELECT * FROM system_contexts WHERE user_id = ? ORDER BY created_at DESC",
                )
                .bind(&user_id)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_ctx()).collect())
            })
        },
    )
    .await
}

pub async fn get_active_system_context(user_id: &str) -> Result<Option<SystemContext>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("system_contexts")
                    .find_one(doc! { "user_id": user_id, "is_active": true }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SystemContextRow>(
                    "SELECT * FROM system_contexts WHERE user_id = ? AND is_active = 1",
                )
                .bind(&user_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_ctx()))
            })
        },
    )
    .await
}

pub async fn get_system_context_by_id(id: &str) -> Result<Option<SystemContext>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("system_contexts")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SystemContextRow>(
                    "SELECT * FROM system_contexts WHERE id = ?",
                )
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_ctx()))
            })
        },
    )
    .await
}

pub async fn create_system_context(ctx: &SystemContext) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let ctx_mongo = ctx.clone();
    let ctx_sqlite = ctx.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(ctx_mongo.id.clone())),
                ("name", Bson::String(ctx_mongo.name.clone())),
                ("content", ctx_mongo.content.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("user_id", Bson::String(ctx_mongo.user_id.clone())),
                ("is_active", Bson::Boolean(ctx_mongo.is_active)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("system_contexts").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO system_contexts (id, name, content, user_id, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
                    .bind(&ctx_sqlite.id)
                    .bind(&ctx_sqlite.name)
                    .bind(&ctx_sqlite.content)
                    .bind(&ctx_sqlite.user_id)
                    .bind(if ctx_sqlite.is_active {1} else {0})
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
}

pub async fn update_system_context(id: &str, updates: &SystemContext) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let updates_mongo = updates.clone();
    let updates_sqlite = updates.clone();
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = Document::new();
                set_doc.insert("name", updates_mongo.name.clone());
                set_doc.insert("content", updates_mongo.content.clone().map(Bson::String).unwrap_or(Bson::Null));
                set_doc.insert("is_active", Bson::Boolean(updates_mongo.is_active));
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("system_contexts").update_one(doc! { "id": id }, doc! { "$set": set_doc }, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE system_contexts SET name = ?, content = ?, is_active = ?, updated_at = ? WHERE id = ?")
                    .bind(&updates_sqlite.name)
                    .bind(&updates_sqlite.content)
                    .bind(if updates_sqlite.is_active {1} else {0})
                    .bind(&now_sqlite)
                    .bind(&id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
}

pub async fn delete_system_context(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("system_contexts")
                    .delete_one(doc! { "id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("system_context_applications")
                    .delete_many(doc! { "system_context_id": &id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM system_contexts WHERE id = ?")
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

pub async fn activate_system_context(context_id: &str, user_id: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    with_db(
        |db| {
            let context_id = context_id.to_string();
            let user_id = user_id.to_string();
            Box::pin(async move {
                db.collection::<Document>("system_contexts")
                    .update_many(
                        doc! { "user_id": &user_id },
                        doc! { "$set": { "is_active": false } },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<Document>("system_contexts")
                    .update_one(
                        doc! { "id": &context_id },
                        doc! { "$set": { "is_active": true, "updated_at": &now_mongo } },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let context_id = context_id.to_string();
            let user_id = user_id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE system_contexts SET is_active = 0 WHERE user_id = ?")
                    .bind(&user_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(
                    "UPDATE system_contexts SET is_active = 1, updated_at = ? WHERE id = ?",
                )
                .bind(&now_sqlite)
                .bind(&context_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
    )
    .await
}

pub async fn get_app_ids_for_system_context(context_id: &str) -> Result<Vec<String>, String> {
    with_db(
        |db| {
            let context_id = context_id.to_string();
            Box::pin(async move {
                let mut cursor = db.collection::<Document>("system_context_applications").find(doc! { "system_context_id": context_id }, None).await.map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    if let Ok(app_id) = doc.get_str("application_id") { out.push(app_id.to_string()); }
                }
                Ok(out)
            })
        },
        |pool| {
            let context_id = context_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query("SELECT application_id FROM system_context_applications WHERE system_context_id = ?")
                    .bind(&context_id)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for row in rows {
                    let app_id: String = row.try_get("application_id").unwrap_or_default();
                    out.push(app_id);
                }
                Ok(out)
            })
        }
    ).await
}

pub async fn set_app_ids_for_system_context(
    context_id: &str,
    app_ids: &[String],
) -> Result<(), String> {
    with_db(
        |db| {
            let context_id = context_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                db.collection::<Document>("system_context_applications").delete_many(doc! { "system_context_id": &context_id }, None).await.map_err(|e| e.to_string())?;
                if !app_ids.is_empty() {
                    let now = chrono::Utc::now().to_rfc3339();
                    let docs: Vec<Document> = app_ids.iter().map(|aid| doc! { "id": format!("{}_{}", context_id, aid), "system_context_id": &context_id, "application_id": aid, "created_at": &now }).collect();
                    db.collection::<Document>("system_context_applications").insert_many(docs, None).await.map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        },
        |pool| {
            let context_id = context_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                sqlx::query("DELETE FROM system_context_applications WHERE system_context_id = ?")
                    .bind(&context_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let now = chrono::Utc::now().to_rfc3339();
                for aid in app_ids {
                    sqlx::query("INSERT INTO system_context_applications (id, system_context_id, application_id, created_at) VALUES (?, ?, ?, ?)")
                        .bind(format!("{}_{}", context_id, aid))
                        .bind(&context_id)
                        .bind(&aid)
                        .bind(&now)
                        .execute(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        }
    ).await
}
