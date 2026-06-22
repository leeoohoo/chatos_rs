use super::*;

pub(super) async fn list_external_mcp_configs(
    State(state): State<AppState>,
) -> Result<Json<Vec<ExternalMcpConfigRecord>>, ApiError> {
    let configs = state
        .external_mcp_config_service
        .list_external_mcp_configs()
        .await
        .map_err(ApiError::bad_request)?;
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
) -> Result<Json<ExternalMcpConfigRecord>, ApiError> {
    state
        .external_mcp_config_service
        .get_external_mcp_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("外部 MCP 配置不存在: {id}")))
}

pub(super) async fn update_external_mcp_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateExternalMcpConfigRequest>,
) -> Result<Json<ExternalMcpConfigRecord>, ApiError> {
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
) -> Result<StatusCode, ApiError> {
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
