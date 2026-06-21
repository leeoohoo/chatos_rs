use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentPrincipal;
use crate::integrations::{
    sync_model_config_delete, sync_model_config_upsert, sync_model_settings,
};
use crate::models::{
    CreateUserModelConfigRequest, UpdateUserModelConfigRequest, UpdateUserModelSettingsRequest,
    UserModelConfigRecord, UserModelSettingsRecord,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{bad_request, forbidden, internal_error, not_found, ApiResult, ApiStatusResult};

#[derive(Debug, Default, Deserialize)]
pub struct UserScopeQuery {
    user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ModelConfigGetQuery {
    include_secret: Option<bool>,
}

pub async fn list_model_configs(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserScopeQuery>,
) -> ApiResult<Vec<serde_json::Value>> {
    let owner_user_id = resolve_target_user_id(&principal, query.user_id.as_deref())?;
    let items = if principal.is_super_admin() && owner_user_id.is_none() {
        state.store.list_user_model_configs(None).await
    } else {
        state.store.list_user_model_configs(owner_user_id.as_deref()).await
    }
    .map_err(internal_error)?;

    Ok(Json(
        items.into_iter()
            .map(|item| model_config_public_value(item, false, None))
            .collect::<Vec<_>>(),
    ))
}

pub async fn create_model_config(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<CreateUserModelConfigRequest>,
) -> ApiResult<serde_json::Value> {
    let owner_user_id = resolve_target_user_id(&principal, input.owner_user_id.as_deref())?
        .ok_or_else(|| bad_request("owner_user_id is required"))?;
    ensure_owner_user_exists(&state, owner_user_id.as_str()).await?;

    let Some(name) = normalize_optional_string(Some(input.name)) else {
        return Err(bad_request("name is required"));
    };
    let provider = normalize_provider_input(input.provider)?;
    let thinking_level =
        normalize_thinking_level_input(provider.as_str(), input.thinking_level.as_deref())?;
    let api_key = normalize_optional_string(input.api_key);
    if api_key.is_none() {
        return Err(bad_request("api_key is required"));
    }
    let model = normalize_optional_string(input.model).unwrap_or_default();

    let now = now_rfc3339();
    let record = UserModelConfigRecord {
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        owner_user_id,
        name,
        provider,
        model,
        thinking_level,
        api_key,
        has_api_key: false,
        base_url: normalize_optional_string(input.base_url),
        enabled: input.enabled.unwrap_or(true),
        supports_images: input.supports_images.unwrap_or(false),
        supports_reasoning: input.supports_reasoning.unwrap_or(false),
        supports_responses: input.supports_responses.unwrap_or(false),
        created_at: now.clone(),
        updated_at: now,
    };

    let saved = state
        .store
        .save_user_model_config(&record)
        .await
        .map_err(internal_error)?;
    let sync_warnings = sync_model_config_upsert(&state, &saved).await;
    Ok(Json(model_config_public_value(saved, false, Some(sync_warnings))))
}

pub async fn get_model_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<ModelConfigGetQuery>,
) -> ApiResult<serde_json::Value> {
    let Some(record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &record)?;
    let include_secret = query.include_secret.unwrap_or(false);
    Ok(Json(model_config_public_value(record, include_secret, None)))
}

pub async fn update_model_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelConfigRequest>,
) -> ApiResult<serde_json::Value> {
    let Some(mut record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &record)?;

    if let Some(name) = input.name {
        let Some(name) = normalize_optional_string(Some(name)) else {
            return Err(bad_request("name is required"));
        };
        record.name = name;
    }
    if let Some(provider) = input.provider {
        record.provider = normalize_provider_input(Some(provider))?;
    }
    if let Some(model) = input.model {
        record.model = normalize_optional_string(Some(model)).unwrap_or_default();
    }
    if input.clear_api_key.unwrap_or(false) {
        record.api_key = None;
    } else if let Some(api_key) = input.api_key {
        record.api_key = normalize_optional_string(Some(api_key));
    }
    if let Some(base_url) = input.base_url {
        record.base_url = normalize_optional_string(Some(base_url));
    }
    if let Some(enabled) = input.enabled {
        record.enabled = enabled;
    }
    if let Some(supports_images) = input.supports_images {
        record.supports_images = supports_images;
    }
    if let Some(supports_reasoning) = input.supports_reasoning {
        record.supports_reasoning = supports_reasoning;
    }
    if let Some(supports_responses) = input.supports_responses {
        record.supports_responses = supports_responses;
    }
    if input.thinking_level.is_some() {
        record.thinking_level =
            normalize_thinking_level_input(record.provider.as_str(), input.thinking_level.as_deref())?;
    }
    record.updated_at = now_rfc3339();

    let saved = state
        .store
        .save_user_model_config(&record)
        .await
        .map_err(internal_error)?;
    let sync_warnings = sync_model_config_upsert(&state, &saved).await;
    Ok(Json(model_config_public_value(saved, false, Some(sync_warnings))))
}

pub async fn delete_model_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiStatusResult {
    let Some(record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &record)?;

    let deleted = state
        .store
        .delete_user_model_config(id.as_str())
        .await
        .map_err(internal_error)?;
    if deleted {
        let _sync_warnings = sync_model_config_delete(&state, id.as_str()).await;
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(not_found("model config not found"))
    }
}

pub async fn get_model_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Query(query): Query<UserScopeQuery>,
) -> ApiResult<serde_json::Value> {
    let user_id = resolve_target_user_id(&principal, query.user_id.as_deref())?
        .ok_or_else(|| bad_request("user_id is required"))?;
    ensure_owner_user_exists(&state, user_id.as_str()).await?;

    let settings = state
        .store
        .get_user_model_settings(user_id.as_str())
        .await
        .map_err(internal_error)?
        .unwrap_or(UserModelSettingsRecord {
            user_id: user_id.clone(),
            memory_summary_model_config_id: None,
            updated_at: now_rfc3339(),
        });

    Ok(Json(model_settings_public_value(settings, None)))
}

pub async fn put_model_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelSettingsRequest>,
) -> ApiResult<serde_json::Value> {
    let user_id = resolve_target_user_id(&principal, input.user_id.as_deref())?
        .ok_or_else(|| bad_request("user_id is required"))?;
    ensure_owner_user_exists(&state, user_id.as_str()).await?;

    if let Some(model_config_id) = input
        .memory_summary_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let Some(model_config) = state
            .store
            .find_user_model_config_by_id(model_config_id)
            .await
            .map_err(internal_error)?
        else {
            return Err(bad_request("memory_summary_model_config_id does not exist"));
        };
        if model_config.owner_user_id != user_id {
            return Err(forbidden(
                "memory_summary_model_config_id does not belong to the target user",
            ));
        }
        if model_config.model.trim().is_empty() {
            return Err(bad_request(
                "memory_summary_model_config_id requires a concrete model name",
            ));
        }
    }

    let settings = UserModelSettingsRecord {
        user_id,
        memory_summary_model_config_id: normalize_optional_string(input.memory_summary_model_config_id),
        updated_at: now_rfc3339(),
    };
    let saved = state
        .store
        .save_user_model_settings(&settings)
        .await
        .map_err(internal_error)?;
    let sync_warnings = sync_model_settings(&state, &saved).await;
    Ok(Json(model_settings_public_value(saved, Some(sync_warnings))))
}

fn resolve_target_user_id(
    principal: &CurrentPrincipal,
    requested_user_id: Option<&str>,
) -> Result<Option<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let requested_user_id = requested_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if principal.is_super_admin() {
        return Ok(requested_user_id.or_else(|| principal.user_id.clone()));
    }
    match (principal.user_id.as_deref(), requested_user_id.as_deref()) {
        (Some(current), Some(requested)) if current != requested => {
            Err(forbidden("cannot access another user's model config"))
        }
        (Some(current), _) => Ok(Some(current.to_string())),
        _ => Err(not_found("current user not found")),
    }
}

fn ensure_model_access(
    principal: &CurrentPrincipal,
    record: &UserModelConfigRecord,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    if principal.is_super_admin() || principal.user_id.as_deref() == Some(record.owner_user_id.as_str()) {
        Ok(())
    } else {
        Err(forbidden("cannot access another user's model config"))
    }
}

async fn ensure_owner_user_exists(
    state: &AppState,
    user_id: &str,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    let Some(user) = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("owner user not found"));
    };
    if !user.enabled {
        return Err(bad_request("owner user is disabled"));
    }
    Ok(())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_provider_input(
    provider: Option<String>,
) -> Result<String, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let provider = provider
        .unwrap_or_else(|| "gpt".to_string())
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");
    match provider.as_str() {
        "openai" | "gpt" => Ok("gpt".to_string()),
        "deepseek" => Ok("deepseek".to_string()),
        "kimi" | "kimik2" | "moonshot" => Ok("kimi".to_string()),
        "minimax" => Ok("minimax".to_string()),
        "openai_compatible" => Ok("openai_compatible".to_string()),
        _ => Err(bad_request(
            "provider only supports gpt / deepseek / kimi / minimax / openai_compatible",
        )),
    }
}

fn normalize_thinking_level_input(
    provider: &str,
    value: Option<&str>,
) -> Result<Option<String>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let Some(level) = value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
    else {
        return Ok(None);
    };
    let normalized = match level.to_ascii_lowercase().as_str() {
        "none" | "off" => "none",
        "auto" => "auto",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" | "max" => "xhigh",
        _ => {
            return Err(bad_request(
                "thinking_level only supports none/auto/minimal/low/medium/high/xhigh/max",
            ))
        }
    };
    if provider != "gpt" && provider != "openai_compatible" && normalized == "xhigh" {
        return Ok(Some("high".to_string()));
    }
    Ok(Some(normalized.to_string()))
}

fn model_config_public_value(
    record: UserModelConfigRecord,
    include_secret: bool,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "id": record.id,
        "owner_user_id": record.owner_user_id,
        "name": record.name,
        "provider": record.provider,
        "model": record.model,
        "model_name": record.model,
        "thinking_level": record.thinking_level,
        "has_api_key": record.has_api_key
            || record
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
        "base_url": record.base_url,
        "enabled": record.enabled,
        "supports_images": record.supports_images,
        "supports_reasoning": record.supports_reasoning,
        "supports_responses": record.supports_responses,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    });
    if include_secret {
        value["api_key"] = Value::String(record.api_key.unwrap_or_default());
    }
    if let Some(sync_warnings) = sync_warnings.filter(|items| !items.is_empty()) {
        value["sync_warnings"] = json!(sync_warnings);
    }
    value
}

fn model_settings_public_value(
    record: UserModelSettingsRecord,
    sync_warnings: Option<Vec<String>>,
) -> serde_json::Value {
    let mut value = json!({
        "user_id": record.user_id,
        "memory_summary_model_config_id": record.memory_summary_model_config_id,
        "updated_at": record.updated_at,
    });
    if let Some(sync_warnings) = sync_warnings.filter(|items| !items.is_empty()) {
        value["sync_warnings"] = json!(sync_warnings);
    }
    value
}
