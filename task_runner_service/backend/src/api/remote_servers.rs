use super::*;

pub(super) async fn list_remote_servers(
    State(state): State<AppState>,
) -> Result<Json<Vec<RemoteServerRecord>>, ApiError> {
    let servers = state
        .remote_server_service
        .list_remote_servers()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(servers))
}

pub(super) async fn create_remote_server(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateRemoteServerRequest>,
) -> Result<(StatusCode, Json<RemoteServerRecord>), ApiError> {
    let server = state
        .remote_server_service
        .create_remote_server(input, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(server)))
}

pub(super) async fn get_remote_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<RemoteServerRecord>, ApiError> {
    state
        .remote_server_service
        .get_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))
}

pub(super) async fn update_remote_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateRemoteServerRequest>,
) -> Result<Json<RemoteServerRecord>, ApiError> {
    let server = state
        .remote_server_service
        .update_remote_server(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    Ok(Json(server))
}

pub(super) async fn delete_remote_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    if state
        .remote_server_service
        .delete_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("远程服务器不存在: {id}")))
    }
}

pub(super) async fn test_remote_server_draft(
    State(state): State<AppState>,
    Json(input): Json<TestRemoteServerRequest>,
) -> Result<Json<RemoteServerTestResponse>, ApiError> {
    let result = state
        .remote_server_service
        .test_remote_server_draft(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

pub(super) async fn test_remote_server_saved(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<RemoteServerTestResponse>, ApiError> {
    let result = state
        .remote_server_service
        .test_remote_server_saved(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    Ok(Json(result))
}
