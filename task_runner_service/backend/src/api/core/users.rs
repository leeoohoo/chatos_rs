use super::*;

pub(in crate::api) async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserSummaryRecord>>, ApiError> {
    let users = state
        .auth_service
        .list_users()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(users))
}

pub(in crate::api) async fn create_user(
    State(state): State<AppState>,
    Json(input): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserSummaryRecord>), ApiError> {
    let user = state
        .auth_service
        .create_user(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(user)))
}

pub(in crate::api) async fn update_user(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateUserRequest>,
) -> Result<Json<UserSummaryRecord>, ApiError> {
    let user = state
        .auth_service
        .update_user(&id, input, &current_user)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("用户不存在: {id}")))?;
    Ok(Json(user))
}

pub(in crate::api) async fn delete_user(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    if state
        .auth_service
        .delete_user(&id, &current_user)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("用户不存在: {id}")))
    }
}
