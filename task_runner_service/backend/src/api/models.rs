use super::*;

pub(super) async fn list_model_configs(
    State(state): State<AppState>,
) -> Result<Json<Vec<ModelConfigRecord>>, ApiError> {
    let models = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(models))
}

pub(super) async fn create_model_config(
    State(state): State<AppState>,
    Json(input): Json<CreateModelConfigRequest>,
) -> Result<(StatusCode, Json<ModelConfigRecord>), ApiError> {
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
) -> Result<Json<ModelConfigRecord>, ApiError> {
    state
        .model_config_service
        .get_model_config(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))
}

pub(super) async fn update_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateModelConfigRequest>,
) -> Result<Json<ModelConfigRecord>, ApiError> {
    let model = state
        .model_config_service
        .update_model_config(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("模型配置不存在: {id}")))?;
    Ok(Json(model))
}

pub(super) async fn delete_model_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
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
    Json(input): Json<TestModelConfigRequest>,
) -> Result<Json<ModelConfigTestResponse>, ApiError> {
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
) -> Result<Json<ModelCatalogResponse>, ApiError> {
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
    Json(input): Json<PreviewModelCatalogRequest>,
) -> Result<Json<ModelCatalogResponse>, ApiError> {
    let result = state
        .model_config_service
        .preview_model_catalog(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

pub(super) async fn list_model_config_usage(
    State(state): State<AppState>,
) -> Result<Json<Vec<ModelConfigUsageRecord>>, ApiError> {
    let usage = state
        .model_config_service
        .usage_stats()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(usage))
}
