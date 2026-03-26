use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::core::remote_connection_error_codes::remote_sftp_codes;

use super::super::transfer_helpers::{RemoteTransferErrorCode, TransferJobError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RemoteSftpApiError {
    BadRequest { code: &'static str, error: String },
    NotFound { code: &'static str, error: String },
    RequestTimeout { code: &'static str, error: String },
}

impl RemoteSftpApiError {
    pub(super) fn bad_request(error: impl Into<String>) -> Self {
        Self::bad_request_with_code(remote_sftp_codes::BAD_REQUEST, error)
    }

    pub(super) fn bad_request_with_code(code: &'static str, error: impl Into<String>) -> Self {
        Self::BadRequest {
            code,
            error: error.into(),
        }
    }

    pub(super) fn not_found_with_code(code: &'static str, error: impl Into<String>) -> Self {
        Self::NotFound {
            code,
            error: error.into(),
        }
    }

    pub(super) fn request_timeout_with_code(code: &'static str, error: impl Into<String>) -> Self {
        Self::RequestTimeout {
            code,
            error: error.into(),
        }
    }

    pub(super) fn remote_error(error: impl Into<String>) -> Self {
        Self::bad_request_with_code(remote_sftp_codes::REMOTE_ERROR, error)
    }

    pub(super) fn into_response(self) -> (StatusCode, Json<Value>) {
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

pub(super) fn map_remote_listing_error(error: String) -> RemoteSftpApiError {
    if error.contains("目录不存在") {
        return RemoteSftpApiError::bad_request_with_code(remote_sftp_codes::INVALID_PATH, error);
    }
    RemoteSftpApiError::remote_error(error)
}
