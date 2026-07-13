// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::{collect_map_sorted_desc, collect_string_field};
use crate::models::system_context::SystemContext;
use crate::repositories::db::{
    doc_from_pairs, mongo_delete_many_doc, mongo_delete_one_doc, mongo_find_one_doc,
    mongo_insert_doc, mongo_update_many_set_doc, mongo_update_set_doc, to_doc, with_db,
};

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
    with_db(|db| {
        let user_id = user_id.to_string();
        Box::pin(async move {
            let cursor = db
                .collection::<Document>("system_contexts")
                .find(doc! { "user_id": user_id }, None)
                .await
                .map_err(|e| e.to_string())?;
            let items: Vec<SystemContext> =
                collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                    .await?;
            Ok(items)
        })
    })
    .await
}

pub async fn get_active_system_context(user_id: &str) -> Result<Option<SystemContext>, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(
                db,
                "system_contexts",
                doc! { "user_id": user_id, "is_active": true },
            )
            .await?;
            Ok(doc.and_then(|d| normalize_doc(&d)))
        })
    })
    .await
}

pub async fn get_system_context_by_id(id: &str) -> Result<Option<SystemContext>, String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(db, "system_contexts", doc! { "id": id }).await?;
            Ok(doc.and_then(|d| normalize_doc(&d)))
        })
    })
    .await
}

pub async fn create_system_context(ctx: &SystemContext) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let ctx_mongo = ctx.clone();
    with_db(|db| {
        let doc = to_doc(doc_from_pairs(vec![
            ("id", Bson::String(ctx_mongo.id.clone())),
            ("name", Bson::String(ctx_mongo.name.clone())),
            (
                "content",
                crate::core::values::optional_string_bson(ctx_mongo.content.clone()),
            ),
            ("user_id", Bson::String(ctx_mongo.user_id.clone())),
            ("is_active", Bson::Boolean(ctx_mongo.is_active)),
            ("created_at", Bson::String(now_mongo.clone())),
            ("updated_at", Bson::String(now_mongo.clone())),
        ]));
        Box::pin(async move {
            mongo_insert_doc(db, "system_contexts", doc).await?;
            Ok(())
        })
    })
    .await
}

pub async fn update_system_context(id: &str, updates: &SystemContext) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let updates_mongo = updates.clone();
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            let mut set_doc = Document::new();
            set_doc.insert("name", updates_mongo.name.clone());
            set_doc.insert(
                "content",
                crate::core::values::optional_string_bson(updates_mongo.content.clone()),
            );
            set_doc.insert("is_active", Bson::Boolean(updates_mongo.is_active));
            set_doc.insert("updated_at", now_mongo.clone());
            mongo_update_set_doc(db, "system_contexts", doc! { "id": id }, set_doc).await?;
            Ok(())
        })
    })
    .await
}

pub async fn delete_system_context(id: &str) -> Result<(), String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            mongo_delete_one_doc(db, "system_contexts", doc! { "id": &id }).await?;
            mongo_delete_many_doc(
                db,
                "system_context_applications",
                doc! { "system_context_id": &id },
            )
            .await?;
            Ok(())
        })
    })
    .await
}

pub async fn activate_system_context(context_id: &str, user_id: &str) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    with_db(|db| {
        let context_id = context_id.to_string();
        let user_id = user_id.to_string();
        Box::pin(async move {
            mongo_update_many_set_doc(
                db,
                "system_contexts",
                doc! { "user_id": &user_id },
                doc! { "is_active": false },
            )
            .await?;
            mongo_update_set_doc(
                db,
                "system_contexts",
                doc! { "id": &context_id },
                doc! { "is_active": true, "updated_at": &now_mongo },
            )
            .await?;
            Ok(())
        })
    })
    .await
}

pub async fn get_app_ids_for_system_context(context_id: &str) -> Result<Vec<String>, String> {
    with_db(|db| {
        let context_id = context_id.to_string();
        Box::pin(async move {
            let cursor = db
                .collection::<Document>("system_context_applications")
                .find(doc! { "system_context_id": context_id }, None)
                .await
                .map_err(|e| e.to_string())?;
            collect_string_field(cursor, "application_id").await
        })
    })
    .await
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
                mongo_delete_many_doc(
                    db,
                    "system_context_applications",
                    doc! { "system_context_id": &context_id },
                )
                .await?;
                if !app_ids.is_empty() {
                    let now = crate::core::time::now_rfc3339();
                    let docs: Vec<Document> = app_ids.iter().map(|aid| doc! { "id": format!("{}_{}", context_id, aid), "system_context_id": &context_id, "application_id": aid, "created_at": &now }).collect();
                    db.collection::<Document>("system_context_applications").insert_many(docs, None).await.map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        }).await
}
