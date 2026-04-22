use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use std::path::Path as FsPath;
use tokio::time::Duration;

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::core::validation::normalize_non_empty;
use crate::models::remote_connection::RemoteConnectionService;

use super::super::transfer_helpers::{run_scp_download_typed, run_scp_upload_typed};
use super::super::{
    join_remote_path, normalize_remote_path, remote_parent_path, resolve_jump_connection_snapshot,
    shell_quote,
};
use super::contracts::{
    SftpDeleteRequest, SftpDownloadRequest, SftpListQuery, SftpMkdirRequest, SftpRenameRequest,
    SftpUploadRequest,
};
use super::errors::{map_remote_listing_error, RemoteSftpApiError};
use super::support::{
    ensure_local_target_parent_dir_exists, fetch_remote_entries, require_non_empty_field,
    validate_mkdir_name, verification_code_from_headers,
};

pub(crate) async fn list_remote_sftp_entries(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<SftpListQuery>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => return RemoteSftpApiError::remote_error(err).into_response(),
    };

    let path = normalize_non_empty(query.path)
        .or(resolved_connection.default_remote_path.clone())
        .unwrap_or_else(|| ".".to_string());
    let verification_code = verification_code_from_headers(&headers);

    match fetch_remote_entries(&resolved_connection, path.as_str(), verification_code.as_deref()).await {
        Ok(entries) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "path": normalize_remote_path(path.as_str()),
                    "parent": remote_parent_path(path.as_str()),
                    "entries": entries
                })),
            )
        }
        Err(err) => map_remote_listing_error(err).into_response(),
    }
}

pub(crate) async fn upload_file_to_remote(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpUploadRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let local_path = match require_non_empty_field(req.local_path, "local_path") {
        Ok(v) => v,
        Err(err) => return err.into_response(),
    };
    let remote_path = match require_non_empty_field(req.remote_path, "remote_path") {
        Ok(v) => v,
        Err(err) => return err.into_response(),
    };
    let verification_code = verification_code_from_headers(&headers);

    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => return RemoteSftpApiError::remote_error(err).into_response(),
    };

    let local = FsPath::new(&local_path);
    if !local.exists() || !local.is_file() {
        return RemoteSftpApiError::bad_request_with_code(
            crate::core::remote_connection_error_codes::remote_sftp_codes::INVALID_PATH,
            "本地文件不存在或不是文件",
        )
        .into_response();
    }

    match run_scp_upload_typed(
        &resolved_connection,
        local_path.as_str(),
        remote_path.as_str(),
        verification_code.as_deref(),
    )
    .await
    {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::from(err).into_response(),
    }
}

pub(crate) async fn download_file_from_remote(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpDownloadRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let remote_path = match require_non_empty_field(req.remote_path, "remote_path") {
        Ok(v) => v,
        Err(err) => return err.into_response(),
    };
    let local_path = match require_non_empty_field(req.local_path, "local_path") {
        Ok(v) => v,
        Err(err) => return err.into_response(),
    };
    let verification_code = verification_code_from_headers(&headers);

    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => return RemoteSftpApiError::remote_error(err).into_response(),
    };

    if let Err(err) = ensure_local_target_parent_dir_exists(local_path.as_str()) {
        return err.into_response();
    }

    match run_scp_download_typed(
        &resolved_connection,
        remote_path.as_str(),
        local_path.as_str(),
        verification_code.as_deref(),
    )
    .await
    {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::from(err).into_response(),
    }
}

pub(crate) async fn create_remote_directory(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpMkdirRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let parent = normalize_non_empty(req.parent_path).unwrap_or_else(|| ".".to_string());
    let name = match require_non_empty_field(req.name, "name") {
        Ok(name) => name,
        Err(err) => return err.into_response(),
    };

    if let Err(err) = validate_mkdir_name(name.as_str()) {
        return err.into_response();
    }

    let target_path = join_remote_path(parent.as_str(), name.as_str());
    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => return RemoteSftpApiError::remote_error(err).into_response(),
    };

    let script = format!("mkdir -p {}", shell_quote(target_path.as_str()));
    let verification_code = verification_code_from_headers(&headers);
    match super::super::run_ssh_command_with_verification(
        &resolved_connection,
        script.as_str(),
        Duration::from_secs(20),
        verification_code.as_deref(),
    )
    .await
    {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "success": true, "path": target_path })),
            )
        }
        Err(err) => RemoteSftpApiError::remote_error(err).into_response(),
    }
}

pub(crate) async fn rename_remote_entry(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpRenameRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let from_path = match require_non_empty_field(req.from_path, "from_path") {
        Ok(path) => path,
        Err(err) => return err.into_response(),
    };
    let to_path = match require_non_empty_field(req.to_path, "to_path") {
        Ok(path) => path,
        Err(err) => return err.into_response(),
    };

    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => return RemoteSftpApiError::remote_error(err).into_response(),
    };

    let script = format!(
        "mv {} {}",
        shell_quote(from_path.as_str()),
        shell_quote(to_path.as_str())
    );
    let verification_code = verification_code_from_headers(&headers);
    match super::super::run_ssh_command_with_verification(
        &resolved_connection,
        script.as_str(),
        Duration::from_secs(20),
        verification_code.as_deref(),
    )
    .await
    {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::remote_error(err).into_response(),
    }
}

pub(crate) async fn delete_remote_entry(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpDeleteRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let path = match require_non_empty_field(req.path, "path") {
        Ok(path) => path,
        Err(err) => return err.into_response(),
    };
    let recursive = req.recursive.unwrap_or(false);

    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => return RemoteSftpApiError::remote_error(err).into_response(),
    };

    let quoted = shell_quote(path.as_str());
    let script = if recursive {
        format!("rm -rf {}", quoted)
    } else {
        format!(
            "if [ -d {p} ]; then rmdir {p}; else rm -f {p}; fi",
            p = quoted
        )
    };
    let verification_code = verification_code_from_headers(&headers);

    match super::super::run_ssh_command_with_verification(
        &resolved_connection,
        script.as_str(),
        Duration::from_secs(20),
        verification_code.as_deref(),
    )
    .await
    {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::remote_error(err).into_response(),
    }
}
