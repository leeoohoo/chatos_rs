// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineModelProfile, UpsertEngineModelProfileRequest};

use super::common::model_profile_collection;

pub async fn list_model_profiles(db: &Db) -> Result<Vec<EngineModelProfile>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"is_default": -1, "enabled": -1, "updated_at": -1, "id": 1})
        .build();
    let cursor = model_profile_collection(db)
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;
    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn list_model_profiles_by_owner(
    db: &Db,
    owner_user_id: &str,
) -> Result<Vec<EngineModelProfile>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"is_default": -1, "enabled": -1, "updated_at": -1, "id": 1})
        .build();
    let cursor = model_profile_collection(db)
        .find(owner_exact_filter(owner_user_id))
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;
    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub async fn count_model_profiles(db: &Db) -> Result<i64, String> {
    model_profile_collection(db)
        .count_documents(doc! {})
        .await
        .map(|count| count as i64)
        .map_err(|err| err.to_string())
}

pub async fn get_model_profile_by_id(
    db: &Db,
    id: &str,
) -> Result<Option<EngineModelProfile>, String> {
    model_profile_collection(db)
        .find_one(doc! {"id": id})
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_model_profile_by_id_for_owner(
    db: &Db,
    id: &str,
    owner_user_id: &str,
) -> Result<Option<EngineModelProfile>, String> {
    model_profile_collection(db)
        .find_one(doc! {"id": id, "owner_user_id": owner_user_id})
        .await
        .map_err(|err| err.to_string())
}

pub async fn get_active_model_profile(
    db: &Db,
    owner_user_id: Option<&str>,
) -> Result<Option<EngineModelProfile>, String> {
    if let Some(owner_user_id) = normalize_owner(owner_user_id) {
        if let Some(profile) = model_profile_collection(db)
            .find_one(doc! {
                "owner_user_id": owner_user_id,
                "is_default": true,
                "enabled": true
            })
            .sort(doc! {"updated_at": -1, "id": 1})
            .await
            .map_err(|err| err.to_string())?
        {
            return Ok(Some(profile));
        }

        if let Some(profile) = model_profile_collection(db)
            .find_one(doc! {"owner_user_id": owner_user_id, "enabled": true})
            .sort(doc! {"updated_at": -1, "id": 1})
            .await
            .map_err(|err| err.to_string())?
        {
            return Ok(Some(profile));
        }
    }

    if let Some(profile) = model_profile_collection(db)
        .find_one(global_enabled_filter(Some(true), Some(true)))
        .sort(doc! {"updated_at": -1, "id": 1})
        .await
        .map_err(|err| err.to_string())?
    {
        return Ok(Some(profile));
    }

    model_profile_collection(db)
        .find_one(global_enabled_filter(Some(true), None))
        .sort(doc! {"updated_at": -1, "id": 1})
        .await
        .map_err(|err| err.to_string())
}

async fn clear_other_default_model_profiles(
    db: &Db,
    keep_id: &str,
    owner_user_id: Option<&str>,
) -> Result<(), String> {
    let mut filter = owner_scope_filter(owner_user_id);
    filter.insert("id", doc! {"$ne": keep_id});
    filter.insert("is_default", true);
    model_profile_collection(db)
        .update_many(filter, doc! {"$set": {"is_default": false}})
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn create_model_profile(
    db: &Db,
    owner_user_id: Option<&str>,
    owner_username: Option<&str>,
    req: UpsertEngineModelProfileRequest,
) -> Result<EngineModelProfile, String> {
    let UpsertEngineModelProfileRequest {
        id,
        name,
        provider,
        model,
        base_url,
        api_key,
        supports_images,
        supports_reasoning,
        supports_responses,
        temperature,
        thinking_level,
        model_request_max_retries,
        is_default,
        enabled,
    } = req;
    let now = now_rfc3339();
    let id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let model_request_max_retries = model_request_max_retries.unwrap_or(5);
    if model_request_max_retries > 10 {
        return Err("model_request_max_retries must be between 0 and 10".to_string());
    }
    let enabled = enabled.unwrap_or(true);
    let is_default = is_default.unwrap_or(false);
    let profile = EngineModelProfile {
        id: id.clone(),
        owner_user_id: normalize_owner(owner_user_id).map(ToOwned::to_owned),
        owner_username: normalize_owner(owner_username).map(ToOwned::to_owned),
        name,
        provider,
        model,
        base_url: base_url.unwrap_or(None),
        api_key: api_key.unwrap_or(None),
        supports_images: supports_images.unwrap_or(false),
        supports_reasoning: supports_reasoning.unwrap_or(false),
        supports_responses: supports_responses.unwrap_or(false),
        temperature,
        thinking_level,
        model_request_max_retries,
        is_default,
        enabled,
        created_at: now.clone(),
        updated_at: now,
    };

    if profile.is_default {
        clear_other_default_model_profiles(db, id.as_str(), profile.owner_user_id.as_deref())
            .await?;
    }

    model_profile_collection(db)
        .insert_one(profile.clone())
        .await
        .map_err(|err| err.to_string())?;
    Ok(profile)
}

pub async fn update_model_profile(
    db: &Db,
    id: &str,
    req: UpsertEngineModelProfileRequest,
) -> Result<Option<EngineModelProfile>, String> {
    let Some(existing) = get_model_profile_by_id(db, id).await? else {
        return Ok(None);
    };
    let is_default = req.is_default.unwrap_or(existing.is_default);
    let enabled = req.enabled.unwrap_or(existing.enabled);
    let model_request_max_retries = req
        .model_request_max_retries
        .unwrap_or(existing.model_request_max_retries);
    if model_request_max_retries > 10 {
        return Err("model_request_max_retries must be between 0 and 10".to_string());
    }

    let updated = EngineModelProfile {
        id: existing.id,
        owner_user_id: existing.owner_user_id,
        owner_username: existing.owner_username,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url.unwrap_or(existing.base_url),
        api_key: req.api_key.unwrap_or(existing.api_key),
        supports_images: req.supports_images.unwrap_or(existing.supports_images),
        supports_reasoning: req
            .supports_reasoning
            .unwrap_or(existing.supports_reasoning),
        supports_responses: req
            .supports_responses
            .unwrap_or(existing.supports_responses),
        temperature: req.temperature.or(existing.temperature),
        thinking_level: req.thinking_level.or(existing.thinking_level),
        model_request_max_retries,
        is_default,
        enabled,
        created_at: existing.created_at,
        updated_at: now_rfc3339(),
    };

    if updated.is_default {
        clear_other_default_model_profiles(db, id, updated.owner_user_id.as_deref()).await?;
    }

    model_profile_collection(db)
        .replace_one(doc! {"id": id}, updated.clone())
        .await
        .map_err(|err| err.to_string())?;
    Ok(Some(updated))
}

pub async fn delete_model_profile(db: &Db, id: &str) -> Result<bool, String> {
    let result = model_profile_collection(db)
        .delete_one(doc! {"id": id})
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.deleted_count > 0)
}

fn normalize_owner(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn owner_exact_filter(owner_user_id: &str) -> Document {
    doc! {"owner_user_id": owner_user_id}
}

fn owner_scope_filter(owner_user_id: Option<&str>) -> Document {
    match normalize_owner(owner_user_id) {
        Some(owner_user_id) => owner_exact_filter(owner_user_id),
        None => global_owner_filter(),
    }
}

fn global_owner_filter() -> Document {
    doc! {
        "$or": [
            {"owner_user_id": {"$exists": false}},
            {"owner_user_id": Bson::Null},
            {"owner_user_id": ""}
        ]
    }
}

fn global_enabled_filter(enabled: Option<bool>, is_default: Option<bool>) -> Document {
    let clauses = vec![
        doc! {"owner_user_id": {"$exists": false}},
        doc! {"owner_user_id": Bson::Null},
        doc! {"owner_user_id": ""},
    ];
    let mut filter = doc! { "$or": clauses };
    if let Some(enabled) = enabled {
        filter.insert("enabled", enabled);
    }
    if let Some(is_default) = is_default {
        filter.insert("is_default", is_default);
    }
    filter
}
