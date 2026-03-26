use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{AiModelConfig, UpsertAiModelConfigRequest};
use crate::repositories::now_rfc3339;

fn model_collection(db: &Db) -> mongodb::Collection<AiModelConfig> {
    db.collection::<AiModelConfig>("ai_model_configs")
}

pub async fn list_model_configs(db: &Db, user_id: &str) -> Result<Vec<AiModelConfig>, String> {
    let options = FindOptions::builder().sort(doc! {"updated_at": -1}).build();
    let cursor = model_collection(db)
        .find(doc! {"user_id": user_id})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_model_config_by_id(db: &Db, id: &str) -> Result<Option<AiModelConfig>, String> {
    model_collection(db)
        .find_one(doc! {"id": id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_model_config(
    db: &Db,
    req: UpsertAiModelConfigRequest,
) -> Result<AiModelConfig, String> {
    let now = now_rfc3339();
    let model = AiModelConfig {
        id: Uuid::new_v4().to_string(),
        user_id: req.user_id,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url,
        api_key: req.api_key,
        supports_images: if req.supports_images.unwrap_or(false) {
            1
        } else {
            0
        },
        supports_reasoning: if req.supports_reasoning.unwrap_or(false) {
            1
        } else {
            0
        },
        supports_responses: if req.supports_responses.unwrap_or(false) {
            1
        } else {
            0
        },
        temperature: req.temperature,
        thinking_level: req.thinking_level,
        enabled: if req.enabled.unwrap_or(true) { 1 } else { 0 },
        created_at: now.clone(),
        updated_at: now,
    };

    model_collection(db)
        .insert_one(model.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(model)
}

pub async fn update_model_config(
    db: &Db,
    id: &str,
    req: UpsertAiModelConfigRequest,
) -> Result<Option<AiModelConfig>, String> {
    let existing = get_model_config_by_id(db, id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };

    let updated = AiModelConfig {
        id: existing.id,
        user_id: req.user_id,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url.or(existing.base_url),
        api_key: req.api_key.or(existing.api_key),
        supports_images: if req.supports_images.unwrap_or(existing.supports_images == 1) {
            1
        } else {
            0
        },
        supports_reasoning: if req
            .supports_reasoning
            .unwrap_or(existing.supports_reasoning == 1)
        {
            1
        } else {
            0
        },
        supports_responses: if req
            .supports_responses
            .unwrap_or(existing.supports_responses == 1)
        {
            1
        } else {
            0
        },
        temperature: req.temperature.or(existing.temperature),
        thinking_level: req.thinking_level.or(existing.thinking_level),
        enabled: if req.enabled.unwrap_or(existing.enabled == 1) {
            1
        } else {
            0
        },
        created_at: existing.created_at,
        updated_at: now_rfc3339(),
    };

    model_collection(db)
        .replace_one(doc! {"id": id}, updated.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(updated))
}

pub async fn delete_model_config(db: &Db, id: &str) -> Result<bool, String> {
    let result = model_collection(db)
        .delete_one(doc! {"id": id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn delete_user_model_configs(db: &Db, user_id: &str) -> Result<(), String> {
    model_collection(db)
        .delete_many(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
