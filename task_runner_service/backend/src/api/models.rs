// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};

#[derive(Debug, Deserialize)]
struct UserServiceOwnerSummary {
    id: String,
    username: String,
    display_name: String,
}

#[derive(Debug, Clone)]
struct OwnerIdentity {
    username: String,
    display_name: String,
}

pub(super) async fn list_model_configs(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<ModelConfigRecord>>, ApiError> {
    let models = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(ApiError::bad_request)?;
    let models = models
        .into_iter()
        .filter(|model| {
            owned_resource_visible_to_user(model.owner_user_id.as_deref(), &current_user)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let models = attach_model_owner_labels(&state, &current_user, models).await;
    Ok(Json(models))
}

pub(super) async fn create_model_config(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateModelConfigRequest>,
) -> Result<(StatusCode, Json<ModelConfigRecord>), ApiError> {
    require_admin_user(&current_user)?;
    let model = state
        .model_config_service
        .create_model_config(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(model)))
}

pub(super) async fn get_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ModelConfigRecord>, ApiError> {
    let model = state
        .model_config_service
        .get_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    ensure_owned_resource_access(model.owner_user_id.as_deref(), &current_user)?;
    let model = attach_model_owner_labels(&state, &current_user, vec![model])
        .await
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(model))
}

pub(super) async fn update_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateModelConfigRequest>,
) -> Result<Json<ModelConfigRecord>, ApiError> {
    let existing = state
        .model_config_service
        .get_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    ensure_owned_resource_access(existing.owner_user_id.as_deref(), &current_user)?;
    let model = state
        .model_config_service
        .update_model_config(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    let model = attach_model_owner_labels(&state, &current_user, vec![model])
        .await
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(model))
}

pub(super) async fn delete_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .model_config_service
        .get_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    ensure_owned_resource_access(existing.owner_user_id.as_deref(), &current_user)?;
    if state
        .model_config_service
        .delete_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("模型配置不存在: {id}")))
    }
}

pub(super) async fn test_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<TestModelConfigRequest>,
) -> Result<Json<ModelConfigTestResponse>, ApiError> {
    let existing = state
        .model_config_service
        .get_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    ensure_owned_resource_access(existing.owner_user_id.as_deref(), &current_user)?;
    let result = state
        .model_config_service
        .test_model_config(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(result))
}

pub(super) async fn list_model_catalog(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ModelCatalogResponse>, ApiError> {
    let existing = state
        .model_config_service
        .get_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    ensure_owned_resource_access(existing.owner_user_id.as_deref(), &current_user)?;
    let result = state
        .model_config_service
        .list_model_catalog(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(result))
}

pub(super) async fn preview_model_catalog(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<PreviewModelCatalogRequest>,
) -> Result<Json<ModelCatalogResponse>, ApiError> {
    require_admin_user(&current_user)?;
    let result = state
        .model_config_service
        .preview_model_catalog(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

pub(super) async fn list_model_config_usage(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<ModelConfigUsageRecord>>, ApiError> {
    let usage = state
        .model_config_service
        .usage_stats()
        .await
        .map_err(ApiError::bad_request)?;
    if current_user.is_admin() {
        return Ok(Json(usage));
    }
    let visible_model_ids = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|model| {
            owned_resource_visible_to_user(model.owner_user_id.as_deref(), &current_user)
                .unwrap_or(false)
        })
        .map(|model| model.id)
        .collect::<HashSet<_>>();
    let usage = usage
        .into_iter()
        .filter(|item| visible_model_ids.contains(item.model_config_id.as_str()))
        .collect::<Vec<_>>();
    Ok(Json(usage))
}

async fn attach_model_owner_labels(
    state: &AppState,
    current_user: &CurrentUser,
    mut models: Vec<ModelConfigRecord>,
) -> Vec<ModelConfigRecord> {
    if models.is_empty() {
        return models;
    }

    let mut owners = std::collections::HashMap::<String, OwnerIdentity>::new();
    if let Some(owner_user_id) = current_user.effective_owner_user_id() {
        owners.insert(
            owner_user_id.to_string(),
            OwnerIdentity {
                username: current_user
                    .effective_owner_username()
                    .unwrap_or(current_user.username.as_str())
                    .to_string(),
                display_name: current_user
                    .effective_owner_display_name()
                    .unwrap_or(current_user.display_name.as_str())
                    .to_string(),
            },
        );
    }

    let needs_user_service = models.iter().any(|model| {
        let Some(owner_user_id) = normalized_text(model.owner_user_id.as_deref()) else {
            return false;
        };
        let has_username = normalized_text(model.owner_username.as_deref()).is_some();
        let has_display_name = normalized_text(model.owner_display_name.as_deref()).is_some();
        (!has_username || !has_display_name) && !owners.contains_key(owner_user_id.as_str())
    });

    if needs_user_service {
        match load_owner_identities_from_user_service(state).await {
            Ok(user_service_owners) => owners.extend(user_service_owners),
            Err(err) => tracing::warn!(
                error = err.as_str(),
                "failed to hydrate model config owner labels from user_service"
            ),
        }
    }

    for model in models.iter_mut() {
        model.owner_user_id = normalized_text(model.owner_user_id.as_deref());
        model.owner_username = normalized_text(model.owner_username.as_deref());
        model.owner_display_name = normalized_text(model.owner_display_name.as_deref());
        let Some(owner_user_id) = model.owner_user_id.as_deref() else {
            continue;
        };
        let Some(owner) = owners.get(owner_user_id) else {
            continue;
        };
        if model.owner_username.is_none() {
            model.owner_username = Some(owner.username.clone());
        }
        if model.owner_display_name.is_none() {
            model.owner_display_name = Some(owner.display_name.clone());
        }
    }

    models
}

async fn load_owner_identities_from_user_service(
    state: &AppState,
) -> Result<std::collections::HashMap<String, OwnerIdentity>, String> {
    let access_token = crate::auth::get_current_access_token()
        .ok_or_else(|| "missing user_service access token".to_string())?;
    let endpoint = format!(
        "{}/api/users",
        state
            .config
            .user_service_base_url
            .trim()
            .trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(state.config.user_service_request_timeout)
        .build()
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let response = client
        .get(endpoint)
        .bearer_auth(access_token.trim())
        .send()
        .await
        .map_err(|err| format!("user_service /api/users request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let message =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!(
            "user_service /api/users failed: {status} {}",
            message.trim()
        ));
    }
    let users =
        read_response_json_limited::<Vec<UserServiceOwnerSummary>>(response, JSON_BODY_LIMIT_BYTES)
            .await
            .map_err(|err| format!("parse user_service /api/users response failed: {err}"))?;
    Ok(users
        .into_iter()
        .map(|user| {
            (
                user.id,
                OwnerIdentity {
                    username: user.username,
                    display_name: user.display_name,
                },
            )
        })
        .collect())
}

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
