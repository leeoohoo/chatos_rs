// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Query, State};
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::integrations::sync_model_settings;
use crate::models::{
    UpdateUserModelSettingsRequest, UserModelConfigRecord, UserModelSettingsRecord,
};
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
            project_management_agent_model_config_id: None,
            project_management_agent_thinking_level: None,
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

    let current = state
        .store
        .get_user_model_settings(user_id.as_str())
        .await
        .map_err(internal_error)?
        .unwrap_or(UserModelSettingsRecord {
            user_id: user_id.clone(),
            memory_summary_model_config_id: None,
            memory_summary_thinking_level: None,
            project_management_agent_model_config_id: None,
            project_management_agent_thinking_level: None,
            updated_at: now_rfc3339(),
        });
    let memory_summary_model_config_id = resolve_optional_update(
        input.memory_summary_model_config_id,
        current.memory_summary_model_config_id,
    );
    let project_management_agent_model_config_id = resolve_optional_update(
        input.project_management_agent_model_config_id,
        current.project_management_agent_model_config_id,
    );
    let memory_summary_thinking_level_input = resolve_thinking_level_update(
        input.memory_summary_thinking_level,
        current.memory_summary_thinking_level,
        memory_summary_model_config_id.as_deref(),
    );
    let project_management_agent_thinking_level_input = resolve_thinking_level_update(
        input.project_management_agent_thinking_level,
        current.project_management_agent_thinking_level,
        project_management_agent_model_config_id.as_deref(),
    );

    let memory_summary_model_config = validate_settings_model_config(
        &state,
        user_id.as_str(),
        memory_summary_model_config_id.as_deref(),
        "memory_summary_model_config_id",
    )
    .await?;
    let project_management_agent_model_config = validate_settings_model_config(
        &state,
        user_id.as_str(),
        project_management_agent_model_config_id.as_deref(),
        "project_management_agent_model_config_id",
    )
    .await?;
    let memory_summary_provider = memory_summary_model_config
        .as_ref()
        .map(|model_config| model_config.provider.as_str())
        .unwrap_or("gpt");
    let project_management_agent_provider = project_management_agent_model_config
        .as_ref()
        .map(|model_config| model_config.provider.as_str())
        .unwrap_or("gpt");

    let settings = UserModelSettingsRecord {
        user_id,
        memory_summary_model_config_id,
        memory_summary_thinking_level: normalize_thinking_level_input(
            memory_summary_provider,
            memory_summary_thinking_level_input.as_deref(),
        )?,
        project_management_agent_model_config_id,
        project_management_agent_thinking_level: normalize_thinking_level_input(
            project_management_agent_provider,
            project_management_agent_thinking_level_input.as_deref(),
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

fn resolve_optional_update(
    input: Option<Option<String>>,
    current: Option<String>,
) -> Option<String> {
    match input {
        Some(value) => normalize_optional_string(value),
        None => current,
    }
}

fn resolve_thinking_level_update(
    input: Option<Option<String>>,
    current: Option<String>,
    model_config_id: Option<&str>,
) -> Option<String> {
    match input {
        Some(value) => normalize_optional_string(value),
        None if model_config_id.is_none() => None,
        None => current,
    }
}

async fn validate_settings_model_config(
    state: &AppState,
    user_id: &str,
    model_config_id: Option<&str>,
    field_name: &str,
) -> Result<Option<UserModelConfigRecord>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let Some(model_config_id) = model_config_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let Some(model_config) = state
        .store
        .find_user_model_config_by_id(model_config_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(bad_request(format!("{field_name} does not exist")));
    };
    if model_config.owner_user_id != user_id {
        return Err(forbidden(format!(
            "{field_name} does not belong to the target user"
        )));
    }
    if model_config.model.trim().is_empty() {
        return Err(bad_request(format!(
            "{field_name} requires a concrete model name"
        )));
    }
    Ok(Some(model_config))
}
