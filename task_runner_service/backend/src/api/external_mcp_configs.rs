// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_external_mcp_configs(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<ExternalMcpConfigRecord>>, ApiError> {
    let configs = state
        .external_mcp_config_service
        .list_external_mcp_configs()
        .await
        .map_err(ApiError::bad_request)?;
    let configs = configs
        .into_iter()
        .filter(|config| {
            owned_resource_visible_to_user(
                resource_owner_or_creator(
                    config.owner_user_id.as_deref(),
                    config.creator_user_id.as_deref(),
                ),
                &current_user,
            )
            .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    Ok(Json(configs))
}

pub(super) async fn create_external_mcp_config(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateExternalMcpConfigRequest>,
) -> Result<(StatusCode, Json<ExternalMcpConfigRecord>), ApiError> {
    let config = state
        .external_mcp_config_service
        .create_external_mcp_config(input, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(config)))
}

pub(super) async fn get_external_mcp_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ExternalMcpConfigRecord>, ApiError> {
    let config = state
        .external_mcp_config_service
        .get_external_mcp_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("外部 MCP 配置不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            config.owner_user_id.as_deref(),
            config.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    Ok(Json(config))
}

pub(super) async fn update_external_mcp_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateExternalMcpConfigRequest>,
) -> Result<Json<ExternalMcpConfigRecord>, ApiError> {
    let existing = state
        .external_mcp_config_service
        .get_external_mcp_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("外部 MCP 配置不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            existing.owner_user_id.as_deref(),
            existing.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    let config = state
        .external_mcp_config_service
        .update_external_mcp_config(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("外部 MCP 配置不存在: {id}")))?;
    Ok(Json(config))
}

pub(super) async fn delete_external_mcp_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .external_mcp_config_service
        .get_external_mcp_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("外部 MCP 配置不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            existing.owner_user_id.as_deref(),
            existing.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    if state
        .external_mcp_config_service
        .delete_external_mcp_config(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("外部 MCP 配置不存在: {id}")))
    }
}
