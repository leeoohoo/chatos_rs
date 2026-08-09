// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query},
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
use crate::core::validation::normalize_non_empty;
use crate::models::remote_connection::RemoteConnectionService;

use super::super::resolve_jump_connection_snapshot;
use super::contracts::{
    SftpDeleteRequest, SftpDownloadRequest, SftpListQuery, SftpMkdirRequest, SftpRenameRequest,
    SftpUploadRequest,
};
use super::errors::RemoteSftpApiError;
use super::support::{
    require_non_empty_field, validate_mkdir_name, verification_code_from_headers,
};

pub(crate) async fn list_remote_sftp_entries(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<SftpListQuery>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(value) => value,
        Err(error) => return map_remote_connection_access_error(error),
    };
    let resolved = match resolve_jump_connection_snapshot(&connection).await {
        Ok(value) => value,
        Err(error) => return RemoteSftpApiError::remote_error(error).into_response(),
    };
    let path = normalize_non_empty(query.path)
        .or(resolved.default_remote_path.clone())
        .unwrap_or_else(|| ".".to_string());
    match remote_sftp_via_connector(
        &resolved,
        "list",
        json!({ "path": path }),
        verification_code_from_headers(&headers).as_deref(),
        Duration::from_secs(30),
    )
    .await
    {
        Ok(value) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(value))
        }
        Err(error) => error,
    }
}

pub(crate) async fn upload_file_to_remote(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpUploadRequest>,
) -> (StatusCode, Json<Value>) {
    start_path_transfer(auth, headers, id, "upload", req.local_path, req.remote_path).await
}

pub(crate) async fn download_file_from_remote(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpDownloadRequest>,
) -> (StatusCode, Json<Value>) {
    start_path_transfer(
        auth,
        headers,
        id,
        "download",
        req.local_path,
        req.remote_path,
    )
    .await
}

async fn start_path_transfer(
    auth: AuthUser,
    headers: HeaderMap,
    id: String,
    direction: &str,
    local_path: Option<String>,
    remote_path: Option<String>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(value) => value,
        Err(error) => return map_remote_connection_access_error(error),
    };
    let local_path = match require_non_empty_field(local_path, "local_path") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let remote_path = match require_non_empty_field(remote_path, "remote_path") {
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

pub(crate) async fn create_remote_directory(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpMkdirRequest>,
) -> (StatusCode, Json<Value>) {
    let parent = normalize_non_empty(req.parent_path).unwrap_or_else(|| ".".to_string());
    let name = match require_non_empty_field(req.name, "name") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = validate_mkdir_name(name.as_str()) {
        return error.into_response();
    }
    mutate(
        auth,
        headers,
        id,
        "mkdir",
        json!({ "parent_path": parent, "name": name }),
    )
    .await
}

pub(crate) async fn rename_remote_entry(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpRenameRequest>,
) -> (StatusCode, Json<Value>) {
    let from_path = match require_non_empty_field(req.from_path, "from_path") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let to_path = match require_non_empty_field(req.to_path, "to_path") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    mutate(
        auth,
        headers,
        id,
        "rename",
        json!({ "from_path": from_path, "to_path": to_path }),
    )
    .await
}

pub(crate) async fn delete_remote_entry(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpDeleteRequest>,
) -> (StatusCode, Json<Value>) {
    let path = match require_non_empty_field(req.path, "path") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    mutate(
        auth,
        headers,
        id,
        "delete",
        json!({ "path": path, "recursive": req.recursive.unwrap_or(false) }),
    )
    .await
}

async fn mutate(
    auth: AuthUser,
    headers: HeaderMap,
    id: String,
    operation: &str,
    payload: Value,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(value) => value,
        Err(error) => return map_remote_connection_access_error(error),
    };
    let resolved = match resolve_jump_connection_snapshot(&connection).await {
        Ok(value) => value,
        Err(error) => return RemoteSftpApiError::remote_error(error).into_response(),
    };
    match remote_sftp_via_connector(
        &resolved,
        operation,
        payload,
        verification_code_from_headers(&headers).as_deref(),
        Duration::from_secs(30),
    )
    .await
    {
        Ok(value) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(value))
        }
        Err(error) => error,
    }
}
