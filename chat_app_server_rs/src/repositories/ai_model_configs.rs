use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::collect_map_sorted_desc;
use crate::core::sql_query::build_select_all_with_optional_user_id;
use crate::db::{self, Database};
use crate::models::ai_model_config::{AiModelConfig, AiModelConfigRow};
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};
use crate::utils::model_config::normalize_provider;

fn normalize_doc(doc: &Document) -> Option<AiModelConfig> {
    let provider_raw = doc.get_str("provider").unwrap_or("openai").to_string();
    let provider = normalize_provider(&provider_raw);
    Some(AiModelConfig {
        id: doc.get_str("id").ok()?.to_string(),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        name: doc.get_str("name").ok()?.to_string(),
        provider,
        model: doc.get_str("model").ok()?.to_string(),
        thinking_level: doc.get_str("thinking_level").ok().map(|s| s.to_string()),
        api_key: doc.get_str("api_key").ok().map(|s| s.to_string()),
        base_url: doc.get_str("base_url").ok().map(|s| s.to_string()),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        supports_images: doc.get_bool("supports_images").unwrap_or(false),
        supports_reasoning: doc.get_bool("supports_reasoning").unwrap_or(false),
        supports_responses: doc.get_bool("supports_responses").unwrap_or(false),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

async fn has_legacy_ai_model_configs_storage() -> Result<bool, String> {
    let db = db::get_db().await?;
    match db.as_ref() {
        Database::Mongo { db, .. } => {
            let names = db
                .list_collection_names(None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(names.iter().any(|name| name == "ai_model_configs"))
        }
        Database::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='ai_model_configs' LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.is_some())
        }
    }
}

pub async fn list_ai_model_configs(user_id: Option<&str>) -> Result<Vec<AiModelConfig>, String> {
    if !has_legacy_ai_model_configs_storage().await? {
        return Ok(Vec::new());
    }
    with_db(
        |db| {
            let user_id = user_id.map(|item| item.to_string());
            Box::pin(async move {
                let filter = match user_id {
                    Some(user_id) => doc! { "user_id": user_id },
                    None => Document::new(),
                };
                let cursor = db
                    .collection::<Document>("ai_model_configs")
                    .find(filter, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let items: Vec<AiModelConfig> =
                    collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                        .await?;
                Ok(items)
            })
        },
        |pool| {
            let user_id = user_id.map(|item| item.to_string());
            Box::pin(async move {
                let query = build_select_all_with_optional_user_id(
                    "ai_model_configs",
                    user_id.is_some(),
                    true,
                );
                let mut sql = sqlx::query_as::<_, AiModelConfigRow>(&query);
                if let Some(user_id) = user_id {
                    sql = sql.bind(user_id);
                }
                let rows = sql.fetch_all(pool).await.map_err(|e| e.to_string())?;
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

pub async fn create_ai_model_config(config: &AiModelConfig) -> Result<AiModelConfig, String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let config_mongo = config.clone();
    let config_sqlite = config.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(config_mongo.id.clone())),
                (
                    "user_id",
                    crate::core::values::optional_string_bson(config_mongo.user_id.clone()),
                ),
                ("name", Bson::String(config_mongo.name.clone())),
                ("provider", Bson::String(config_mongo.provider.clone())),
                ("model", Bson::String(config_mongo.model.clone())),
                (
                    "thinking_level",
                    crate::core::values::optional_string_bson(config_mongo.thinking_level.clone()),
                ),
                (
                    "api_key",
                    crate::core::values::optional_string_bson(config_mongo.api_key.clone()),
                ),
                (
                    "base_url",
                    crate::core::values::optional_string_bson(config_mongo.base_url.clone()),
                ),
                ("enabled", Bson::Boolean(config_mongo.enabled)),
                ("supports_images", Bson::Boolean(config_mongo.supports_images)),
                (
                    "supports_reasoning",
                    Bson::Boolean(config_mongo.supports_reasoning),
                ),
                (
                    "supports_responses",
                    Bson::Boolean(config_mongo.supports_responses),
                ),
                ("created_at", Bson::String(now_mongo.clone())),
                ("updated_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("ai_model_configs")
                    .insert_one(doc, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(AiModelConfig {
                    created_at: now_mongo.clone(),
                    updated_at: now_mongo,
                    ..config_mongo
                })
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO ai_model_configs (id, user_id, name, provider, model, thinking_level, api_key, base_url, enabled, supports_images, supports_reasoning, supports_responses, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&config_sqlite.id)
                .bind(&config_sqlite.user_id)
                .bind(&config_sqlite.name)
                .bind(&config_sqlite.provider)
                .bind(&config_sqlite.model)
                .bind(&config_sqlite.thinking_level)
                .bind(&config_sqlite.api_key)
                .bind(&config_sqlite.base_url)
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.enabled))
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.supports_images))
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.supports_reasoning))
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.supports_responses))
                .bind(&now_sqlite)
                .bind(&now_sqlite)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(AiModelConfig {
                    created_at: now_sqlite.clone(),
                    updated_at: now_sqlite,
                    ..config_sqlite
                })
            })
        },
    )
    .await
}

pub async fn update_ai_model_config(id: &str, config: &AiModelConfig) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let config_mongo = config.clone();
    let config_sqlite = config.clone();
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let mut set_doc = Document::new();
                set_doc.insert(
                    "user_id",
                    crate::core::values::optional_string_bson(config_mongo.user_id.clone()),
                );
                set_doc.insert("name", config_mongo.name.clone());
                set_doc.insert("provider", config_mongo.provider.clone());
                set_doc.insert("model", config_mongo.model.clone());
                set_doc.insert(
                    "thinking_level",
                    crate::core::values::optional_string_bson(config_mongo.thinking_level.clone()),
                );
                set_doc.insert(
                    "api_key",
                    crate::core::values::optional_string_bson(config_mongo.api_key.clone()),
                );
                set_doc.insert(
                    "base_url",
                    crate::core::values::optional_string_bson(config_mongo.base_url.clone()),
                );
                set_doc.insert("enabled", Bson::Boolean(config_mongo.enabled));
                set_doc.insert(
                    "supports_images",
                    Bson::Boolean(config_mongo.supports_images),
                );
                set_doc.insert(
                    "supports_reasoning",
                    Bson::Boolean(config_mongo.supports_reasoning),
                );
                set_doc.insert(
                    "supports_responses",
                    Bson::Boolean(config_mongo.supports_responses),
                );
                set_doc.insert("updated_at", now_mongo.clone());
                db.collection::<Document>("ai_model_configs")
                    .update_one(doc! { "id": id }, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query(
                    "UPDATE ai_model_configs SET user_id = ?, name = ?, provider = ?, model = ?, thinking_level = ?, api_key = ?, base_url = ?, enabled = ?, supports_images = ?, supports_reasoning = ?, supports_responses = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&config_sqlite.user_id)
                .bind(&config_sqlite.name)
                .bind(&config_sqlite.provider)
                .bind(&config_sqlite.model)
                .bind(&config_sqlite.thinking_level)
                .bind(&config_sqlite.api_key)
                .bind(&config_sqlite.base_url)
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.enabled))
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.supports_images))
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.supports_reasoning))
                .bind(crate::core::values::bool_to_sqlite_int(config_sqlite.supports_responses))
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

pub async fn delete_ai_model_config(id: &str) -> Result<(), String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                db.collection::<Document>("ai_model_configs")
                    .delete_one(doc! { "id": &id }, None)
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
