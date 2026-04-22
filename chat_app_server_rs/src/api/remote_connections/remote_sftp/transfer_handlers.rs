use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use std::path::Path as FsPath;

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::core::remote_connection_error_codes::remote_sftp_codes;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

use super::super::request_normalize::normalize_transfer_direction;
use super::super::transfer_helpers::{
    estimate_local_total_bytes_typed, run_sftp_transfer_job_typed,
};
use super::super::transfer_manager::get_sftp_transfer_manager;
use super::super::resolve_jump_connection_snapshot;
use super::contracts::SftpTransferStartRequest;
use super::errors::RemoteSftpApiError;
use super::support::{
    ensure_local_target_parent_dir_exists, require_non_empty_field, verification_code_from_headers,
};

pub(crate) async fn start_sftp_transfer(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SftpTransferStartRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let resolved_connection = match resolve_jump_connection_snapshot(&connection).await {
        Ok(connection) => connection,
        Err(err) => return RemoteSftpApiError::remote_error(err).into_response(),
    };

    let direction = match normalize_transfer_direction(req.direction) {
        Ok(v) => v,
        Err(err) => {
            return RemoteSftpApiError::bad_request_with_code(
                remote_sftp_codes::INVALID_ARGUMENT,
                err,
            )
            .into_response()
        }
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

    if direction == "upload" {
        let source = FsPath::new(local_path.as_str());
        if !source.exists() {
            return RemoteSftpApiError::bad_request_with_code(
                remote_sftp_codes::INVALID_PATH,
                "本地路径不存在",
            )
            .into_response();
        }
        if !source.is_file() && !source.is_dir() {
            return RemoteSftpApiError::bad_request_with_code(
                remote_sftp_codes::INVALID_PATH,
                "本地路径必须是文件或目录",
            )
            .into_response();
        }
    } else if let Err(err) = ensure_local_target_parent_dir_exists(local_path.as_str()) {
        return err.into_response();
    }

    let total_estimated = if direction == "upload" {
        estimate_local_total_bytes_typed(FsPath::new(local_path.as_str())).ok()
    } else {
        None
    };
    let current_path = if direction == "upload" {
        Some(local_path.clone())
    } else {
        Some(remote_path.clone())
    };
    let transfer_manager = get_sftp_transfer_manager();
    let status = transfer_manager.create(
        resolved_connection.id.as_str(),
        direction.as_str(),
        total_estimated,
        current_path,
    );

    let connection_for_task = resolved_connection.clone();
    let transfer_id_for_task = status.id.clone();
    let direction_for_task = direction.clone();
    let local_for_task = local_path.clone();
    let remote_for_task = remote_path.clone();
    let verification_code_for_task = verification_code.clone();
    tokio::spawn(async move {
        run_sftp_transfer_task(
            connection_for_task,
            transfer_id_for_task,
            direction_for_task,
            local_for_task,
            remote_for_task,
            verification_code_for_task,
        )
        .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(status).unwrap_or(Value::Null)),
    )
}

pub(crate) async fn get_sftp_transfer_status(
    auth: AuthUser,
    Path((id, transfer_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let transfer_manager = get_sftp_transfer_manager();
    match transfer_manager.get_for_connection(transfer_id.as_str(), connection.id.as_str()) {
        Some(status) => (
            StatusCode::OK,
            Json(serde_json::to_value(status).unwrap_or(Value::Null)),
        ),
        None => RemoteSftpApiError::not_found_with_code(
            remote_sftp_codes::TRANSFER_NOT_FOUND,
            "传输任务不存在",
        )
        .into_response(),
    }
}

pub(crate) async fn cancel_sftp_transfer(
    auth: AuthUser,
    Path((id, transfer_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let transfer_manager = get_sftp_transfer_manager();
    let accepted = transfer_manager
        .request_cancel_for_connection(transfer_id.as_str(), connection.id.as_str());
    if !accepted {
        return RemoteSftpApiError::bad_request_with_code(
            remote_sftp_codes::TRANSFER_NOT_ACTIVE,
            "传输任务不存在或已结束",
        )
        .into_response();
    }

    match transfer_manager.get_for_connection(transfer_id.as_str(), connection.id.as_str()) {
        Some(status) => (
            StatusCode::OK,
            Json(serde_json::to_value(status).unwrap_or(Value::Null)),
        ),
        None => RemoteSftpApiError::not_found_with_code(
            remote_sftp_codes::TRANSFER_NOT_FOUND,
            "传输任务不存在",
        )
        .into_response(),
    }
}

async fn run_sftp_transfer_task(
    connection: RemoteConnection,
    transfer_id: String,
    direction: String,
    local_path: String,
    remote_path: String,
    verification_code: Option<String>,
) {
    let transfer_manager = get_sftp_transfer_manager();
    transfer_manager.set_running(transfer_id.as_str());

    let transfer_manager_for_blocking = transfer_manager.clone();
    let connection_for_blocking = connection.clone();
    let direction_for_blocking = direction.clone();
    let local_for_blocking = local_path.clone();
    let remote_for_blocking = remote_path.clone();
    let verification_code_for_blocking = verification_code.clone();
    let transfer_id_for_blocking = transfer_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        run_sftp_transfer_job_typed(
            &connection_for_blocking,
            transfer_id_for_blocking.as_str(),
            direction_for_blocking.as_str(),
            local_for_blocking.as_str(),
            remote_for_blocking.as_str(),
            verification_code_for_blocking.as_deref(),
            transfer_manager_for_blocking.as_ref(),
        )
    })
    .await;

    match result {
        Ok(Ok(message)) => transfer_manager.set_done(transfer_id.as_str(), message),
        Ok(Err(err)) if err.is_cancelled() => transfer_manager.set_cancelled(transfer_id.as_str()),
        Ok(Err(err)) => transfer_manager.set_error(transfer_id.as_str(), err.to_string()),
        Err(err) => {
            transfer_manager.set_error(transfer_id.as_str(), format!("传输线程执行失败: {err}"))
        }
    }

    let _ = RemoteConnectionService::touch(&connection.id).await;
}
