// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Query, State};
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::integrations::sync_model_settings;
use crate::models::{UpdateUserModelSettingsRequest, UserModelSettingsRecord};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::super::{bad_request, forbidden, internal_error, ApiResult};
use super::access::{ensure_owner_user_exists, resolve_target_user_id};
use super::contracts::UserScopeQuery;
use super::model_values::model_settings_public_value;
use super::normalization::{normalize_optional_string, normalize_thinking_level_input};

pub(in crate::api) async fn get_model_settings(
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
            memory_summary_thinking_level: None,
            updated_at: now_rfc3339(),
        });

    Ok(Json(model_settings_public_value(settings, None)))
}

pub(in crate::api) async fn put_model_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateUserModelSettingsRequest>,
) -> ApiResult<serde_json::Value> {
    let user_id = resolve_target_user_id(&principal, input.user_id.as_deref())?
        .ok_or_else(|| bad_request("user_id is required"))?;
    ensure_owner_user_exists(&state, user_id.as_str()).await?;

    let mut memory_summary_provider = "gpt".to_string();
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
        memory_summary_provider = model_config.provider;
    }

    let settings = UserModelSettingsRecord {
        user_id,
        memory_summary_model_config_id: normalize_optional_string(
            input.memory_summary_model_config_id,
        ),
        memory_summary_thinking_level: normalize_thinking_level_input(
            memory_summary_provider.as_str(),
            input.memory_summary_thinking_level.as_deref(),
        )?,
        updated_at: now_rfc3339(),
    };
    let saved = state
        .store
        .save_user_model_settings(&settings)
        .await
        .map_err(internal_error)?;
    let sync_warnings = sync_model_settings(&state, &saved).await;
    Ok(Json(model_settings_public_value(
        saved,
        Some(sync_warnings),
    )))
}
