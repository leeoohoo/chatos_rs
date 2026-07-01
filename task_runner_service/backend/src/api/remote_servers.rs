// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn list_remote_servers(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<RemoteServerRecord>>, ApiError> {
    let servers = state
        .remote_server_service
        .list_remote_servers()
        .await
        .map_err(ApiError::bad_request)?;
    let servers = servers
        .into_iter()
        .filter(|server| {
            owned_resource_visible_to_user(
                resource_owner_or_creator(
                    server.owner_user_id.as_deref(),
                    server.creator_user_id.as_deref(),
                ),
                &current_user,
            )
            .unwrap_or(false)
        })
        .collect::<Vec<_>>();
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
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<RemoteServerRecord>, ApiError> {
    let server = state
        .remote_server_service
        .get_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            server.owner_user_id.as_deref(),
            server.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    Ok(Json(server))
}

pub(super) async fn update_remote_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateRemoteServerRequest>,
) -> Result<Json<RemoteServerRecord>, ApiError> {
    let existing = state
        .remote_server_service
        .get_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            existing.owner_user_id.as_deref(),
            existing.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
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
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    let existing = state
        .remote_server_service
        .get_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            existing.owner_user_id.as_deref(),
            existing.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
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
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<RemoteServerTestResponse>, ApiError> {
    let existing = state
        .remote_server_service
        .get_remote_server(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    ensure_owned_resource_access(
        resource_owner_or_creator(
            existing.owner_user_id.as_deref(),
            existing.creator_user_id.as_deref(),
        ),
        &current_user,
    )?;
    let result = state
        .remote_server_service
        .test_remote_server_saved(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("远程服务器不存在: {id}")))?;
    Ok(Json(result))
}
