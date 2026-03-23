use axum::http::StatusCode;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::remote_connection_error_codes::remote_sftp_codes;

use super::super::transfer_helpers::{RemoteTransferErrorCode, TransferJobError};
use super::errors::RemoteSftpApiError;
use super::support::{
    ensure_local_target_parent_dir_exists, require_non_empty_field, validate_mkdir_name,
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
    let (status, body) =
        RemoteSftpApiError::not_found_with_code(remote_sftp_codes::TRANSFER_NOT_FOUND, "not found")
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
        RemoteSftpApiError::from(TransferJobError::Timeout("上传超时".to_string())).into_response();
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
