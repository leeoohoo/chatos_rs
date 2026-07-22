// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Query, State};
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::integrations::sync_model_settings;
use crate::models::{
    UpdateUserModelSettingsRequest, UserModelConfigRecord, UserModelSettingsRecord,
    DEFAULT_MODEL_REQUEST_MAX_RETRIES, MAX_MODEL_REQUEST_MAX_RETRIES,
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
            model_request_max_retries: DEFAULT_MODEL_REQUEST_MAX_RETRIES,
            memory_summary_model_config_id: None,
            memory_summary_thinking_level: None,
            project_management_agent_model_config_id: None,
            project_management_agent_thinking_level: None,
            environment_initialization_model_config_id: None,
            environment_initialization_thinking_level: None,
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
            model_request_max_retries: DEFAULT_MODEL_REQUEST_MAX_RETRIES,
            memory_summary_model_config_id: None,
            memory_summary_thinking_level: None,
            project_management_agent_model_config_id: None,
            project_management_agent_thinking_level: None,
            environment_initialization_model_config_id: None,
            environment_initialization_thinking_level: None,
            updated_at: now_rfc3339(),
        });
    let model_request_max_retries = resolve_model_request_max_retries(
        input.model_request_max_retries,
        current.model_request_max_retries,
    )
    .map_err(bad_request)?;
    let memory_summary_model_config_id = resolve_optional_update(
        input.memory_summary_model_config_id,
        current.memory_summary_model_config_id,
    );
    let project_management_agent_model_config_id = resolve_optional_update(
        input.project_management_agent_model_config_id,
        current.project_management_agent_model_config_id,
    );
    let environment_initialization_model_config_id = resolve_optional_update(
        input.environment_initialization_model_config_id,
        current.environment_initialization_model_config_id,
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
    let environment_initialization_thinking_level_input = resolve_thinking_level_update(
        input.environment_initialization_thinking_level,
        current.environment_initialization_thinking_level,
        environment_initialization_model_config_id.as_deref(),
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
    let environment_initialization_model_config = validate_settings_model_config(
        &state,
        user_id.as_str(),
        environment_initialization_model_config_id.as_deref(),
        "environment_initialization_model_config_id",
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
    let environment_initialization_provider = environment_initialization_model_config
        .as_ref()
        .map(|model_config| model_config.provider.as_str())
        .unwrap_or("gpt");

    let settings = UserModelSettingsRecord {
        user_id,
        model_request_max_retries,
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
        environment_initialization_model_config_id,
        environment_initialization_thinking_level: normalize_thinking_level_input(
            environment_initialization_provider,
            environment_initialization_thinking_level_input.as_deref(),
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

fn resolve_model_request_max_retries(input: Option<i64>, current: i64) -> Result<i64, String> {
    let value = input.unwrap_or(current);
    if !(0..=MAX_MODEL_REQUEST_MAX_RETRIES).contains(&value) {
        return Err(format!(
            "model_request_max_retries must be between 0 and {MAX_MODEL_REQUEST_MAX_RETRIES}"
        ));
    }
    Ok(value)
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

#[cfg(test)]
mod tests {
    use super::resolve_model_request_max_retries;

    #[test]
    fn model_request_retry_setting_defaults_to_current_value_and_validates_bounds() {
        assert_eq!(resolve_model_request_max_retries(None, 5), Ok(5));
        assert_eq!(resolve_model_request_max_retries(Some(0), 5), Ok(0));
        assert_eq!(resolve_model_request_max_retries(Some(10), 5), Ok(10));
        assert!(resolve_model_request_max_retries(Some(-1), 5).is_err());
        assert!(resolve_model_request_max_retries(Some(11), 5).is_err());
    }
}
