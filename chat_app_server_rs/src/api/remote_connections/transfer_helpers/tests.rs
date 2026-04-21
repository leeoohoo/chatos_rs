use crate::core::remote_connection_error_codes::remote_sftp_codes;

use super::{RemoteTransferErrorCode, TransferJobError};

#[test]
fn classifies_cancelled_error() {
    let err = TransferJobError::Cancelled;
    assert!(err.is_cancelled());
    assert!(matches!(err, TransferJobError::Cancelled));
    assert_eq!(err.to_string(), "传输已取消");
}

#[test]
fn classifies_io_error() {
    let err = TransferJobError::io("读取本地文件失败: permission denied");
    assert!(!err.is_cancelled());
    assert!(matches!(err, TransferJobError::Io(_)));
    assert_eq!(err.to_string(), "读取本地文件失败: permission denied");
}

#[test]
fn classifies_timeout_error() {
    let err = TransferJobError::timeout("上传超时");
    assert!(!err.is_cancelled());
    assert!(matches!(err, TransferJobError::Timeout(_)));
    assert_eq!(err.to_string(), "上传超时");
}

#[test]
fn classifies_remote_error() {
    let err = TransferJobError::remote("读取远端文件失败: permission denied");
    assert!(!err.is_cancelled());
    assert!(matches!(
        err,
        TransferJobError::Remote {
            code: RemoteTransferErrorCode::PermissionDenied,
            ..
        }
    ));
    assert_eq!(err.to_string(), "读取远端文件失败: permission denied");
}

#[test]
fn classifies_remote_auth_failed_error_code() {
    let err = TransferJobError::remote("ssh authentication failed");
    assert!(matches!(
        err,
        TransferJobError::Remote {
            code: RemoteTransferErrorCode::AuthFailed,
            ..
        }
    ));
}

#[test]
fn classifies_second_factor_required_error_code() {
    let err = TransferJobError::remote("__CHATOS_SECOND_FACTOR_REQUIRED__:SMS OTP");
    assert!(matches!(
        err,
        TransferJobError::Remote {
            code: RemoteTransferErrorCode::SecondFactorRequired,
            ..
        }
    ));
}

#[test]
fn classifies_wrapped_second_factor_required_error_code() {
    let err = TransferJobError::remote(
        "跳板机认证失败：jump_password 认证失败: __CHATOS_SECOND_FACTOR_REQUIRED__:SMS OTP。请检查配置",
    );
    assert!(matches!(
        err,
        TransferJobError::Remote {
            code: RemoteTransferErrorCode::SecondFactorRequired,
            ..
        }
    ));
}

#[test]
fn classifies_remote_path_not_found_error_code() {
    let err = TransferJobError::remote("No such file");
    assert!(matches!(
        err,
        TransferJobError::Remote {
            code: RemoteTransferErrorCode::PathNotFound,
            ..
        }
    ));
}

#[test]
fn classifies_remote_network_disconnected_error_code() {
    let err = TransferJobError::remote("Connection reset by peer");
    assert!(matches!(
        err,
        TransferJobError::Remote {
            code: RemoteTransferErrorCode::NetworkDisconnected,
            ..
        }
    ));
}

#[test]
fn classifies_remote_protocol_error_code_as_default() {
    let err = TransferJobError::remote("unknown sftp failure");
    assert!(matches!(
        err,
        TransferJobError::Remote {
            code: RemoteTransferErrorCode::Protocol,
            ..
        }
    ));
}

#[test]
fn maps_remote_transfer_error_codes_to_api_codes() {
    assert_eq!(
        RemoteTransferErrorCode::AuthFailed.as_api_code(),
        remote_sftp_codes::REMOTE_AUTH_FAILED
    );
    assert_eq!(
        RemoteTransferErrorCode::SecondFactorRequired.as_api_code(),
        remote_sftp_codes::SECOND_FACTOR_REQUIRED
    );
    assert_eq!(
        RemoteTransferErrorCode::PathNotFound.as_api_code(),
        remote_sftp_codes::REMOTE_PATH_NOT_FOUND
    );
    assert_eq!(
        RemoteTransferErrorCode::PermissionDenied.as_api_code(),
        remote_sftp_codes::REMOTE_PERMISSION_DENIED
    );
    assert_eq!(
        RemoteTransferErrorCode::NetworkDisconnected.as_api_code(),
        remote_sftp_codes::REMOTE_NETWORK_DISCONNECTED
    );
    assert_eq!(
        RemoteTransferErrorCode::Protocol.as_api_code(),
        remote_sftp_codes::REMOTE_ERROR
    );
}
