// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Value};
use std::time::Duration;

use crate::api::local_connectors::remote_sftp_via_connector;
use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::core::remote_connection_error_codes::remote_sftp_codes;

use super::super::request_normalize::normalize_transfer_direction;
use super::super::resolve_jump_connection_snapshot;
use super::contracts::SftpTransferStartRequest;
use super::errors::RemoteSftpApiError;
use super::support::{require_non_empty_field, verification_code_from_headers};

pub(crate) async fn start_sftp_transfer(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpTransferStartRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(value) => value,
        Err(error) => return map_remote_connection_access_error(error),
    };
    let direction = match normalize_transfer_direction(req.direction) {
        Ok(value) => value,
        Err(error) => {
            return RemoteSftpApiError::bad_request_with_code(
                remote_sftp_codes::INVALID_ARGUMENT,
                error,
            )
            .into_response()
        }
    };
    let local_path = match require_non_empty_field(req.local_path, "local_path") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let remote_path = match require_non_empty_field(req.remote_path, "remote_path") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let resolved = match resolve_jump_connection_snapshot(&connection).await {
        Ok(value) => value,
        Err(error) => return RemoteSftpApiError::remote_error(error).into_response(),
    };
    match remote_sftp_via_connector(
        &resolved,
        "transfer_start",
        json!({ "direction": direction, "local_path": local_path, "remote_path": remote_path }),
        verification_code_from_headers(&headers).as_deref(),
        Duration::from_secs(30),
    )
    .await
    {
        Ok(value) => (StatusCode::ACCEPTED, Json(value)),
        Err(error) => error,
    }
}

pub(crate) async fn get_sftp_transfer_status(
    auth: AuthUser,
    Path((id, transfer_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    transfer_control(auth, id, transfer_id, "transfer_status").await
}

pub(crate) async fn cancel_sftp_transfer(
    auth: AuthUser,
    Path((id, transfer_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    transfer_control(auth, id, transfer_id, "transfer_cancel").await
}

async fn transfer_control(
    auth: AuthUser,
    id: String,
    transfer_id: String,
    operation: &str,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(value) => value,
        Err(error) => return map_remote_connection_access_error(error),
    };
    match remote_sftp_via_connector(
        &connection,
        operation,
        json!({ "transfer_id": transfer_id }),
        None,
        Duration::from_secs(20),
    )
    .await
    {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(error) => error,
    }
}
