async fn validate_device_workspace(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
    workspace_id: &str,
) -> Result<(), ApiError> {
    let device = load_owned_device(state, user, device_id, true).await?;
    let workspace = load_owned_workspace(state, user, workspace_id).await?;
    if workspace.device_id != device.id {
        return Err(ApiError::bad_request(
            "Local Connector workspace is not attached to the selected device",
        ));
    }
    if workspace.status == WORKSPACE_STATUS_DISABLED {
        return Err(ApiError::bad_request(
            "Local Connector workspace is disabled",
        ));
    }
    Ok(())
}

async fn ensure_device_active_lease(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
) -> Result<(), ApiError> {
    let active = state
        .store
        .session_holds_active_lease(owner_user_id, device_id)
        .await
        .map_err(ApiError::internal)?;
    if !active {
        return Err(ApiError::service_unavailable(
            "Local Connector device does not hold the active session lease",
        ));
    }
    Ok(())
}

async fn dispatch_companion_relay(
    state: &AppState,
    request: RelayRequest,
    timeout: std::time::Duration,
    client_session_id: &str,
) -> Result<RelayResponse, ApiError> {
    ensure_device_active_lease(
        state,
        request.owner_user_id.as_str(),
        request.device_id.as_str(),
    )
    .await?;
    state
        .relay
        .dispatch_companion(request, timeout, client_session_id)
        .await
        .map_err(relay_error_to_api_error)
}

fn relay_error_to_api_error(error: RelayError) -> ApiError {
    match error {
        RelayError::Offline => ApiError::service_unavailable(error.message()),
        RelayError::Timeout => ApiError::gateway_timeout(error.message()),
        RelayError::TooManyPendingRequests { .. } => ApiError::too_many_requests(error.message()),
        RelayError::Coordination(_) => ApiError::service_unavailable(error.message()),
        RelayError::RequestEncode(_)
        | RelayError::Signing(_)
        | RelayError::DuplicateRequestId(_)
        | RelayError::ResponseChannelClosed => ApiError::bad_gateway(error.message()),
    }
}

fn relay_response_to_http(response: RelayResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    (status, Json(response.body)).into_response()
}

fn required_text(value: Option<String>, field: &str) -> Result<String, ApiError> {
    crate::models::normalize_optional_text(value)
        .ok_or_else(|| ApiError::bad_request(format!("{field} is required and cannot be empty")))
}

#[cfg(test)]
include!("mod_inline_tests.rs");
