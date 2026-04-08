use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::core::remote_connection_error_codes::remote_connection_codes;
use crate::core::user_scope::resolve_user_id;
use crate::models::remote_connection::RemoteConnectionService;

use super::{
    error_payload, get_remote_terminal_manager, internal_error_response, normalize_create_request,
    normalize_update_request, remote_connectivity_error_response, run_remote_connectivity_test,
    CreateRemoteConnectionRequest, DisconnectReason, RemoteConnectionQuery,
    UpdateRemoteConnectionRequest,
};

pub(super) async fn list_remote_connections(
    auth: AuthUser,
    Query(query): Query<RemoteConnectionQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    match RemoteConnectionService::list(Some(user_id)).await {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

pub(super) async fn create_remote_connection(
    auth: AuthUser,
    Json(req): Json<CreateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let normalized = match normalize_create_request(req, Some(user_id)) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                error_payload(err, remote_connection_codes::INVALID_ARGUMENT),
            );
        }
    };

    if let Err(err) = RemoteConnectionService::create(normalized.clone()).await {
        return internal_error_response(
            remote_connection_codes::REMOTE_CONNECTION_CREATE_FAILED,
            err,
        );
    }

    let saved = RemoteConnectionService::get_by_id(&normalized.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(normalized);

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
    )
}

pub(super) async fn test_remote_connection_draft(
    auth: AuthUser,
    Json(req): Json<CreateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let connection = match normalize_create_request(req, Some(user_id)) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                error_payload(err, remote_connection_codes::INVALID_ARGUMENT),
            );
        }
    };

    match run_remote_connectivity_test(&connection).await {
        Ok(result) => (StatusCode::OK, Json(result)),
        Err(err) => remote_connectivity_error_response(err),
    }
}

pub(super) async fn get_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => (
            StatusCode::OK,
            Json(serde_json::to_value(connection).unwrap_or(Value::Null)),
        ),
        Err(err) => map_remote_connection_access_error(err),
    }
}

pub(super) async fn update_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRemoteConnectionRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let normalized = match normalize_update_request(req, existing.clone()) {
        Ok(connection) => connection,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                error_payload(err, remote_connection_codes::INVALID_ARGUMENT),
            );
        }
    };

    if let Err(err) = RemoteConnectionService::update(&id, &normalized).await {
        return internal_error_response(
            remote_connection_codes::REMOTE_CONNECTION_UPDATE_FAILED,
            err,
        );
    }

    match RemoteConnectionService::get_by_id(&id).await {
        Ok(Some(connection)) => (
            StatusCode::OK,
            Json(serde_json::to_value(connection).unwrap_or(Value::Null)),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            error_payload(
                "远端连接不存在",
                remote_connection_codes::REMOTE_CONNECTION_NOT_FOUND,
            ),
        ),
        Err(err) => {
            internal_error_response(remote_connection_codes::REMOTE_CONNECTION_FETCH_FAILED, err)
        }
    }
}

pub(super) async fn delete_remote_connection(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_remote_connection(&id, &auth).await {
        return map_remote_connection_access_error(err);
    }

    let manager = get_remote_terminal_manager();
    manager.close_with_reason(&id, DisconnectReason::ConnectionDeleted);

    match RemoteConnectionService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true, "message": "远端连接已删除" })),
        ),
        Err(err) => internal_error_response(
            remote_connection_codes::REMOTE_CONNECTION_DELETE_FAILED,
            err,
        ),
    }
}

pub(super) async fn disconnect_remote_terminal(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_remote_connection(&id, &auth).await {
        return map_remote_connection_access_error(err);
    }

    let manager = get_remote_terminal_manager();
    let closed = manager.close_with_reason(&id, DisconnectReason::Manual);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "disconnected": closed,
            "message": if closed { "远端终端已断开" } else { "远端终端当前未连接" }
        })),
    )
}

pub(super) async fn test_remote_connection_saved(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    match run_remote_connectivity_test(&connection).await {
        Ok(result) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(result))
        }
        Err(err) => remote_connectivity_error_response(err),
    }
}
