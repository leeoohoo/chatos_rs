use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path as FsPath;
use tokio::time::Duration;

use crate::core::auth::AuthUser;
use crate::core::remote_connection_access::{
    ensure_owned_remote_connection, map_remote_connection_access_error,
};
use crate::core::remote_connection_error_codes::remote_sftp_codes;
use crate::core::validation::normalize_non_empty;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

use super::path_utils::{join_remote_path, normalize_remote_path, remote_parent_path, shell_quote};
use super::request_normalize::normalize_transfer_direction;
use super::transfer_helpers::{
    estimate_local_total_bytes_typed, run_scp_download_typed, run_scp_upload_typed,
    run_sftp_transfer_job_typed, RemoteTransferErrorCode, TransferJobError,
};
use super::transfer_manager::get_sftp_transfer_manager;

#[derive(Debug, Deserialize)]
pub(super) struct SftpListQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SftpUploadRequest {
    local_path: Option<String>,
    remote_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SftpDownloadRequest {
    remote_path: Option<String>,
    local_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SftpTransferStartRequest {
    direction: Option<String>,
    local_path: Option<String>,
    remote_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SftpMkdirRequest {
    parent_path: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SftpRenameRequest {
    from_path: Option<String>,
    to_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SftpDeleteRequest {
    path: Option<String>,
    recursive: Option<bool>,
}

#[derive(Debug, Serialize)]
struct RemoteEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: Option<u64>,
    modified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RemoteSftpApiError {
    BadRequest { code: &'static str, error: String },
    NotFound { code: &'static str, error: String },
    RequestTimeout { code: &'static str, error: String },
}

impl RemoteSftpApiError {
    fn bad_request(error: impl Into<String>) -> Self {
        Self::bad_request_with_code(remote_sftp_codes::BAD_REQUEST, error)
    }

    fn bad_request_with_code(code: &'static str, error: impl Into<String>) -> Self {
        Self::BadRequest {
            code,
            error: error.into(),
        }
    }

    fn not_found_with_code(code: &'static str, error: impl Into<String>) -> Self {
        Self::NotFound {
            code,
            error: error.into(),
        }
    }

    fn request_timeout_with_code(code: &'static str, error: impl Into<String>) -> Self {
        Self::RequestTimeout {
            code,
            error: error.into(),
        }
    }

    fn remote_error(error: impl Into<String>) -> Self {
        Self::bad_request_with_code(remote_sftp_codes::REMOTE_ERROR, error)
    }

    fn into_response(self) -> (StatusCode, Json<Value>) {
        match self {
            Self::BadRequest { code, error } => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error, "code": code })),
            ),
            Self::NotFound { code, error } => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": error, "code": code })),
            ),
            Self::RequestTimeout { code, error } => (
                StatusCode::REQUEST_TIMEOUT,
                Json(serde_json::json!({ "error": error, "code": code })),
            ),
        }
    }
}

impl From<TransferJobError> for RemoteSftpApiError {
    fn from(value: TransferJobError) -> Self {
        match value {
            TransferJobError::Cancelled => {
                Self::bad_request_with_code(remote_sftp_codes::TRANSFER_CANCELLED, "传输已取消")
            }
            TransferJobError::Timeout(message) => {
                Self::request_timeout_with_code(remote_sftp_codes::TIMEOUT, message)
            }
            TransferJobError::Io(message) => {
                Self::bad_request_with_code(remote_sftp_codes::LOCAL_IO_ERROR, message)
            }
            TransferJobError::Remote { code, message } => {
                if code == RemoteTransferErrorCode::NetworkDisconnected {
                    Self::request_timeout_with_code(code.as_api_code(), message)
                } else {
                    Self::bad_request_with_code(code.as_api_code(), message)
                }
            }
            TransferJobError::Message(message) => Self::bad_request(message),
        }
    }
}

fn require_non_empty_field(
    value: Option<String>,
    field_name: &'static str,
) -> Result<String, RemoteSftpApiError> {
    normalize_non_empty(value).ok_or_else(|| {
        RemoteSftpApiError::bad_request_with_code(
            remote_sftp_codes::INVALID_ARGUMENT,
            format!("{field_name} 不能为空"),
        )
    })
}

fn ensure_local_target_parent_dir_exists(local_path: &str) -> Result<(), RemoteSftpApiError> {
    if let Some(parent) = FsPath::new(local_path).parent() {
        if !parent.exists() || !parent.is_dir() {
            return Err(RemoteSftpApiError::bad_request_with_code(
                remote_sftp_codes::INVALID_PATH,
                "本地目标目录不存在",
            ));
        }
    }
    Ok(())
}

fn validate_mkdir_name(name: &str) -> Result<(), RemoteSftpApiError> {
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(RemoteSftpApiError::bad_request_with_code(
            remote_sftp_codes::INVALID_DIRECTORY_NAME,
            "目录名不合法",
        ));
    }
    Ok(())
}

fn map_remote_listing_error(error: String) -> RemoteSftpApiError {
    if error.contains("目录不存在") {
        return RemoteSftpApiError::bad_request_with_code(remote_sftp_codes::INVALID_PATH, error);
    }
    RemoteSftpApiError::remote_error(error)
}

pub(super) async fn list_remote_sftp_entries(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<SftpListQuery>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
    };

    let path = normalize_non_empty(query.path)
        .or(connection.default_remote_path.clone())
        .unwrap_or_else(|| ".".to_string());

    match fetch_remote_entries(&connection, path.as_str()).await {
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

pub(super) async fn upload_file_to_remote(
    auth: AuthUser,
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

    let local = FsPath::new(&local_path);
    if !local.exists() || !local.is_file() {
        return RemoteSftpApiError::bad_request_with_code(
            remote_sftp_codes::INVALID_PATH,
            "本地文件不存在或不是文件",
        )
        .into_response();
    }

    match run_scp_upload_typed(&connection, local_path.as_str(), remote_path.as_str()).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::from(err).into_response(),
    }
}

pub(super) async fn download_file_from_remote(
    auth: AuthUser,
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

    if let Err(err) = ensure_local_target_parent_dir_exists(local_path.as_str()) {
        return err.into_response();
    }

    match run_scp_download_typed(&connection, remote_path.as_str(), local_path.as_str()).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::from(err).into_response(),
    }
}

pub(super) async fn start_sftp_transfer(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SftpTransferStartRequest>,
) -> (StatusCode, Json<Value>) {
    let connection = match ensure_owned_remote_connection(&id, &auth).await {
        Ok(connection) => connection,
        Err(err) => return map_remote_connection_access_error(err),
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
        connection.id.as_str(),
        direction.as_str(),
        total_estimated,
        current_path,
    );

    let connection_for_task = connection.clone();
    let transfer_id_for_task = status.id.clone();
    let direction_for_task = direction.clone();
    let local_for_task = local_path.clone();
    let remote_for_task = remote_path.clone();
    tokio::spawn(async move {
        run_sftp_transfer_task(
            connection_for_task,
            transfer_id_for_task,
            direction_for_task,
            local_for_task,
            remote_for_task,
        )
        .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(status).unwrap_or(Value::Null)),
    )
}

pub(super) async fn get_sftp_transfer_status(
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

pub(super) async fn cancel_sftp_transfer(
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
) {
    let transfer_manager = get_sftp_transfer_manager();
    transfer_manager.set_running(transfer_id.as_str());

    let transfer_manager_for_blocking = transfer_manager.clone();
    let connection_for_blocking = connection.clone();
    let direction_for_blocking = direction.clone();
    let local_for_blocking = local_path.clone();
    let remote_for_blocking = remote_path.clone();
    let transfer_id_for_blocking = transfer_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        run_sftp_transfer_job_typed(
            &connection_for_blocking,
            transfer_id_for_blocking.as_str(),
            direction_for_blocking.as_str(),
            local_for_blocking.as_str(),
            remote_for_blocking.as_str(),
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

pub(super) async fn create_remote_directory(
    auth: AuthUser,
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
    let script = format!("mkdir -p {}", shell_quote(target_path.as_str()));
    match super::run_ssh_command(&connection, script.as_str(), Duration::from_secs(20)).await {
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

pub(super) async fn rename_remote_entry(
    auth: AuthUser,
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

    let script = format!(
        "mv {} {}",
        shell_quote(from_path.as_str()),
        shell_quote(to_path.as_str())
    );
    match super::run_ssh_command(&connection, script.as_str(), Duration::from_secs(20)).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::remote_error(err).into_response(),
    }
}

pub(super) async fn delete_remote_entry(
    auth: AuthUser,
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

    let quoted = shell_quote(path.as_str());
    let script = if recursive {
        format!("rm -rf {}", quoted)
    } else {
        format!(
            "if [ -d {p} ]; then rmdir {p}; else rm -f {p}; fi",
            p = quoted
        )
    };

    match super::run_ssh_command(&connection, script.as_str(), Duration::from_secs(20)).await {
        Ok(_) => {
            let _ = RemoteConnectionService::touch(&connection.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "success": true })))
        }
        Err(err) => RemoteSftpApiError::remote_error(err).into_response(),
    }
}

async fn fetch_remote_entries(
    connection: &RemoteConnection,
    path: &str,
) -> Result<Vec<RemoteEntry>, String> {
    let normalized = normalize_remote_path(path);
    let quoted = shell_quote(normalized.as_str());
    let script = format!(
        "set -e; P={quoted}; if [ ! -d \"$P\" ]; then echo __CHATOS_DIR_NOT_FOUND__; exit 52; fi; cd \"$P\"; find . -mindepth 1 -maxdepth 1 -printf '%P\\t%y\\t%s\\t%T@\\n'"
    );

    let output =
        super::run_ssh_command(connection, script.as_str(), Duration::from_secs(20)).await?;
    if output.contains("__CHATOS_DIR_NOT_FOUND__") {
        return Err("远端目录不存在".to_string());
    }

    let mut entries = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split('\t');
        let name = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }

        let kind = parts.next().unwrap_or("f");
        let size = parts.next().and_then(|s| s.parse::<u64>().ok());
        let modified_at = parts.next().map(|s| s.to_string());
        let is_dir = kind == "d";

        entries.push(RemoteEntry {
            path: join_remote_path(normalized.as_str(), name.as_str()),
            name,
            is_dir,
            size,
            modified_at,
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::core::remote_connection_error_codes::remote_sftp_codes;

    use super::{
        ensure_local_target_parent_dir_exists, require_non_empty_field, validate_mkdir_name,
        RemoteSftpApiError, RemoteTransferErrorCode, TransferJobError,
    };

    #[test]
    fn maps_bad_request_error_to_response() {
        let (status, body) = RemoteSftpApiError::bad_request("invalid path").into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({ "error": "invalid path", "code": remote_sftp_codes::BAD_REQUEST })
        );
    }

    #[test]
    fn maps_not_found_error_to_response() {
        let (status, body) = RemoteSftpApiError::not_found_with_code(
            remote_sftp_codes::TRANSFER_NOT_FOUND,
            "not found",
        )
        .into_response();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body.0,
            json!({
                "error": "not found",
                "code": remote_sftp_codes::TRANSFER_NOT_FOUND
            })
        );
    }

    #[test]
    fn converts_transfer_error_to_bad_request() {
        let (status, body) =
            RemoteSftpApiError::from(TransferJobError::Message("upload failed".to_string()))
                .into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({ "error": "upload failed", "code": remote_sftp_codes::BAD_REQUEST })
        );
    }

    #[test]
    fn converts_transfer_timeout_error_to_request_timeout() {
        let (status, body) =
            RemoteSftpApiError::from(TransferJobError::Timeout("上传超时".to_string()))
                .into_response();
        assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
        assert_eq!(
            body.0,
            json!({ "error": "上传超时", "code": remote_sftp_codes::TIMEOUT })
        );
    }

    #[test]
    fn maps_remote_transfer_auth_failed_to_code() {
        let (status, body) = RemoteSftpApiError::from(TransferJobError::Remote {
            code: RemoteTransferErrorCode::AuthFailed,
            message: "SSH 认证失败".to_string(),
        })
        .into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({
                "error": "SSH 认证失败",
                "code": remote_sftp_codes::REMOTE_AUTH_FAILED
            })
        );
    }

    #[test]
    fn maps_remote_transfer_path_not_found_to_code() {
        let (status, body) = RemoteSftpApiError::from(TransferJobError::Remote {
            code: RemoteTransferErrorCode::PathNotFound,
            message: "远端路径不存在".to_string(),
        })
        .into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({
                "error": "远端路径不存在",
                "code": remote_sftp_codes::REMOTE_PATH_NOT_FOUND
            })
        );
    }

    #[test]
    fn maps_remote_transfer_permission_denied_to_code() {
        let (status, body) = RemoteSftpApiError::from(TransferJobError::Remote {
            code: RemoteTransferErrorCode::PermissionDenied,
            message: "远端权限不足".to_string(),
        })
        .into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({
                "error": "远端权限不足",
                "code": remote_sftp_codes::REMOTE_PERMISSION_DENIED
            })
        );
    }

    #[test]
    fn maps_remote_transfer_network_disconnected_to_timeout_status() {
        let (status, body) = RemoteSftpApiError::from(TransferJobError::Remote {
            code: RemoteTransferErrorCode::NetworkDisconnected,
            message: "远端连接中断".to_string(),
        })
        .into_response();
        assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
        assert_eq!(
            body.0,
            json!({
                "error": "远端连接中断",
                "code": remote_sftp_codes::REMOTE_NETWORK_DISCONNECTED
            })
        );
    }

    #[test]
    fn maps_remote_transfer_protocol_to_remote_error_code() {
        let (status, body) = RemoteSftpApiError::from(TransferJobError::Remote {
            code: RemoteTransferErrorCode::Protocol,
            message: "远端协议错误".to_string(),
        })
        .into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({ "error": "远端协议错误", "code": remote_sftp_codes::REMOTE_ERROR })
        );
    }

    #[test]
    fn rejects_empty_local_path_input() {
        let err = require_non_empty_field(Some("   ".to_string()), "local_path").unwrap_err();
        let (status, body) = err.into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({
                "error": "local_path 不能为空",
                "code": remote_sftp_codes::INVALID_ARGUMENT
            })
        );
    }

    #[test]
    fn rejects_empty_remote_path_input() {
        let err = require_non_empty_field(Some("\n\t".to_string()), "remote_path").unwrap_err();
        let (status, body) = err.into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({
                "error": "remote_path 不能为空",
                "code": remote_sftp_codes::INVALID_ARGUMENT
            })
        );
    }

    #[test]
    fn rejects_missing_local_target_directory() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos();
        let local_path = std::env::temp_dir()
            .join(format!("chatos-missing-parent-{suffix}"))
            .join("download.txt");
        let local_path_str = local_path.to_string_lossy().to_string();

        let err = ensure_local_target_parent_dir_exists(local_path_str.as_str()).unwrap_err();
        let (status, body) = err.into_response();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body.0,
            json!({
                "error": "本地目标目录不存在",
                "code": remote_sftp_codes::INVALID_PATH
            })
        );
    }

    #[test]
    fn rejects_invalid_directory_name() {
        for value in [".", "..", "a/b", "a\\b"] {
            let err = validate_mkdir_name(value).unwrap_err();
            let (status, body) = err.into_response();
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(
                body.0,
                json!({
                    "error": "目录名不合法",
                    "code": remote_sftp_codes::INVALID_DIRECTORY_NAME
                })
            );
        }
    }
}
