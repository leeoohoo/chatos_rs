use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};

use crate::models::ai_model_config::{AiModelConfig, AiModelConfigRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};
use crate::utils::model_config::normalize_provider;

fn normalize_doc(doc: &Document) -> Option<AiModelConfig> {
    let provider_raw = doc.get_str("provider").unwrap_or("openai").to_string();
    let provider = normalize_provider(&provider_raw);
    Some(AiModelConfig {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        provider,
        model: doc.get_str("model").ok()?.to_string(),
        thinking_level: doc.get_str("thinking_level").ok().map(|s| s.to_string()),
        api_key: doc.get_str("api_key").ok().map(|s| s.to_string()),
        base_url: doc.get_str("base_url").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        supports_images: doc.get_bool("supports_images").unwrap_or(false),
        supports_reasoning: doc.get_bool("supports_reasoning").unwrap_or(false),
        supports_responses: doc.get_bool("supports_responses").unwrap_or(false),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn list_ai_model_configs(user_id: Option<String>) -> Result<Vec<AiModelConfig>, String> {
    with_db(
        |db| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let filter = if let Some(uid) = user_id {
                    doc! { "user_id": uid }
                } else {
                    doc! {}
                };
                let mut cursor = db
                    .collection::<Document>("ai_model_configs")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    docs.push(doc);
                }
                let mut items: Vec<AiModelConfig> =
                    docs.into_iter().filter_map(|d| normalize_doc(&d)).collect();
                items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let mut query = "SELECT * FROM ai_model_configs".to_string();
                if user_id.is_some() {
                    query.push_str(" WHERE user_id = ?");
                }
                query.push_str(" ORDER BY created_at DESC");
                let mut q = sqlx::query_as::<_, AiModelConfigRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_model()).collect())
            })
        },
    )
    .await
}

pub async fn get_ai_model_config_by_id(id: &str) -> Result<Option<AiModelConfig>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("ai_model_configs")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, AiModelConfigRow>(
                    "SELECT * FROM ai_model_configs WHERE id = ?",
                )
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_model()))
            })
        },
    )
    .await
}

pub async fn create_ai_model_config(data: &AiModelConfig) -> Result<AiModelConfig, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let data_mongo = data.clone();
    let data_sqlite = data.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(data_mongo.id.clone())),
                ("name", Bson::String(data_mongo.name.clone())),
                ("provider", Bson::String(normalize_provider(&data_mongo.provider))),
                ("model", Bson::String(data_mongo.model.clone())),
                ("thinking_level", data_mongo.thinking_level.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("api_key", data_mongo.api_key.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("base_url", data_mongo.base_url.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("user_id", data_mongo.user_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("enabled", Bson::Boolean(data_mongo.enabled)),
                ("supports_images", Bson::Boolean(data_mongo.supports_images)),
                ("supports_reasoning", Bson::Boolean(data_mongo.supports_reasoning)),
                ("supports_responses", Bson::Boolean(data_mongo.supports_responses)),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("ai_model_configs").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(data_mongo.clone())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO ai_model_configs (id, name, provider, model, thinking_level, api_key, base_url, user_id, enabled, supports_images, supports_reasoning, supports_responses, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(&data_sqlite.id)
                    .bind(&data_sqlite.name)
                    .bind(normalize_provider(&data_sqlite.provider))
                    .bind(&data_sqlite.model)
                    .bind(&data_sqlite.thinking_level)
                    .bind(&data_sqlite.api_key)
                    .bind(&data_sqlite.base_url)
                    .bind(&data_sqlite.user_id)
                    .bind(if data_sqlite.enabled {1} else {0})
                    .bind(if data_sqlite.supports_images {1} else {0})
                    .bind(if data_sqlite.supports_reasoning {1} else {0})
                    .bind(if data_sqlite.supports_responses {1} else {0})
                    .bind(&now_sqlite)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(data_sqlite.clone())
            })
        }
    ).await
}

pub async fn update_ai_model_config(id: &str, updates: &AiModelConfig) -> Result<(), String> {
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
                set_doc.insert("provider", normalize_provider(&updates_mongo.provider));
                set_doc.insert("model", updates_mongo.model.clone());
                set_doc.insert("thinking_level", updates_mongo.thinking_level.clone().map(Bson::String).unwrap_or(Bson::Null));
                set_doc.insert("api_key", updates_mongo.api_key.clone().map(Bson::String).unwrap_or(Bson::Null));
                set_doc.insert("base_url", updates_mongo.base_url.clone().map(Bson::String).unwrap_or(Bson::Null));
                set_doc.insert("enabled", Bson::Boolean(updates_mongo.enabled));
                set_doc.insert("supports_images", Bson::Boolean(updates_mongo.supports_images));
                set_doc.insert("supports_reasoning", Bson::Boolean(updates_mongo.supports_reasoning));
                set_doc.insert("supports_responses", Bson::Boolean(updates_mongo.supports_responses));
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("ai_model_configs").update_one(doc! { "id": id }, doc! { "$set": set_doc }, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("UPDATE ai_model_configs SET name = ?, provider = ?, model = ?, thinking_level = ?, api_key = ?, base_url = ?, enabled = ?, supports_images = ?, supports_reasoning = ?, supports_responses = ?, updated_at = ? WHERE id = ?")
                    .bind(&updates_sqlite.name)
                    .bind(normalize_provider(&updates_sqlite.provider))
                    .bind(&updates_sqlite.model)
                    .bind(&updates_sqlite.thinking_level)
                    .bind(&updates_sqlite.api_key)
                    .bind(&updates_sqlite.base_url)
                    .bind(if updates_sqlite.enabled {1} else {0})
                    .bind(if updates_sqlite.supports_images {1} else {0})
                    .bind(if updates_sqlite.supports_reasoning {1} else {0})
                    .bind(if updates_sqlite.supports_responses {1} else {0})
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

pub async fn delete_ai_model_config(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("ai_model_configs")
                    .delete_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM ai_model_configs WHERE id = ?")
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
