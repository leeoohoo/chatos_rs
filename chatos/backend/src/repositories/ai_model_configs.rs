// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use sqlx::Row;

use crate::core::mongo_cursor::collect_map_sorted_desc;
use crate::core::secrets::{decrypt_optional_secret, encrypt_optional_secret, is_secret_encrypted};
use crate::core::sql_query::build_select_all_with_optional_user_id;
use crate::db::{self, Database};
use crate::models::ai_model_config::{AiModelConfig, AiModelConfigRow};
use crate::repositories::db::{
    doc_from_pairs, mongo_delete_one_doc, mongo_find_one_doc, mongo_insert_doc,
    mongo_update_set_doc, to_doc, with_db,
};
use crate::utils::model_config::normalize_provider;

#[derive(Debug, Default, Clone, Copy)]
pub struct AiModelConfigSecretBackfillReport {
    pub total_count: usize,
    pub migrated_count: usize,
    pub skipped_encrypted_count: usize,
    pub empty_count: usize,
}

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
        task_usage_scenario: None,
        task_thinking_level: None,
        api_key: doc.get_str("api_key").ok().map(|s| s.to_string()),
        has_api_key: false,
        base_url: doc.get_str("base_url").ok().map(|s| s.to_string()),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        supports_images: doc.get_bool("supports_images").unwrap_or(false),
        supports_reasoning: doc.get_bool("supports_reasoning").unwrap_or(false),
        supports_responses: doc.get_bool("supports_responses").unwrap_or(false),
        sync_warnings: Vec::new(),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

fn decrypt_optional_secret_lossy(value: Option<String>) -> Option<String> {
    let fallback = value.clone();
    decrypt_optional_secret(value).unwrap_or(fallback)
}

fn decrypt_model_for_read(mut config: AiModelConfig) -> AiModelConfig {
    config.has_api_key = config
        .api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    config.api_key = decrypt_optional_secret_lossy(config.api_key);
    config
}

fn encrypt_model_for_storage(mut config: AiModelConfig) -> Result<AiModelConfig, String> {
    config.api_key = encrypt_optional_secret(config.api_key)?;
    config.has_api_key = config
        .api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    Ok(config)
}

fn needs_secret_backfill(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| !is_secret_encrypted(value))
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
                        .await?
                        .into_iter()
                        .map(decrypt_model_for_read)
                        .collect();
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
                let mut sql = sqlx::query_as::<_, AiModelConfigRow>(sqlx::AssertSqlSafe(query));
                if let Some(user_id) = user_id {
                    sql = sql.bind(user_id);
                }
                let rows = sql.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .map(|row| decrypt_model_for_read(row.to_model()))
                    .collect())
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
                let doc = mongo_find_one_doc(db, "ai_model_configs", doc! { "id": id }).await?;
                Ok(doc
                    .and_then(|document| normalize_doc(&document))
                    .map(decrypt_model_for_read))
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
                Ok(row.map(|record| decrypt_model_for_read(record.to_model())))
            })
        },
    )
    .await
}

pub async fn create_ai_model_config(config: &AiModelConfig) -> Result<AiModelConfig, String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let config_plain = config.clone();
    let config_mongo = encrypt_model_for_storage(config.clone())?;
    let config_sqlite = encrypt_model_for_storage(config.clone())?;
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
                mongo_insert_doc(db, "ai_model_configs", doc).await?;
                Ok(())
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
                Ok(())
            })
        },
    )
    .await?;

    Ok(AiModelConfig {
        created_at: now.clone(),
        updated_at: now,
        has_api_key: config_plain
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()),
        ..config_plain
    })
}

pub async fn update_ai_model_config(id: &str, config: &AiModelConfig) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let config_mongo = encrypt_model_for_storage(config.clone())?;
    let config_sqlite = encrypt_model_for_storage(config.clone())?;
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
                mongo_update_set_doc(db, "ai_model_configs", doc! { "id": id }, set_doc).await?;
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
                mongo_delete_one_doc(db, "ai_model_configs", doc! { "id": &id }).await?;
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

pub async fn backfill_ai_model_config_secret_storage(
) -> Result<AiModelConfigSecretBackfillReport, String> {
    if !has_legacy_ai_model_configs_storage().await? {
        return Ok(AiModelConfigSecretBackfillReport::default());
    }

    with_db(
        |db| {
            Box::pin(async move {
                let collection = db.collection::<Document>("ai_model_configs");
                let mut cursor = collection
                    .find(Document::new(), None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut report = AiModelConfigSecretBackfillReport::default();

                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    report.total_count += 1;
                    let id = doc
                        .get_str("id")
                        .ok()
                        .map(str::to_string)
                        .unwrap_or_default();
                    let api_key = doc.get_str("api_key").ok().map(str::to_string);
                    let Some(api_key) = api_key else {
                        report.empty_count += 1;
                        continue;
                    };
                    if api_key.trim().is_empty() {
                        report.empty_count += 1;
                        continue;
                    }
                    if !needs_secret_backfill(Some(api_key.as_str())) {
                        report.skipped_encrypted_count += 1;
                        continue;
                    }

                    let encrypted = encrypt_optional_secret(Some(api_key))?.unwrap_or_default();
                    mongo_update_set_doc(
                        db,
                        "ai_model_configs",
                        doc! { "id": id },
                        doc! { "api_key": encrypted },
                    )
                    .await?;
                    report.migrated_count += 1;
                }

                Ok(report)
            })
        },
        |pool| {
            Box::pin(async move {
                let rows = sqlx::query("SELECT id, api_key FROM ai_model_configs")
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut report = AiModelConfigSecretBackfillReport::default();

                for row in rows {
                    report.total_count += 1;
                    let id: String = row.try_get("id").map_err(|e| e.to_string())?;
                    let api_key: Option<String> =
                        row.try_get("api_key").map_err(|e| e.to_string())?;
                    let Some(api_key) = api_key else {
                        report.empty_count += 1;
                        continue;
                    };
                    if api_key.trim().is_empty() {
                        report.empty_count += 1;
                        continue;
                    }
                    if !needs_secret_backfill(Some(api_key.as_str())) {
                        report.skipped_encrypted_count += 1;
                        continue;
                    }

                    let encrypted = encrypt_optional_secret(Some(api_key))?.unwrap_or_default();
                    sqlx::query("UPDATE ai_model_configs SET api_key = ? WHERE id = ?")
                        .bind(encrypted)
                        .bind(id)
                        .execute(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                    report.migrated_count += 1;
                }

                Ok(report)
            })
        },
    )
    .await
}
