use std::fmt::{Display, Formatter};

use crate::core::remote_connection_error_codes::remote_sftp_codes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteTransferErrorCode {
    AuthFailed,
    PathNotFound,
    PermissionDenied,
    NetworkDisconnected,
    Protocol,
}

impl RemoteTransferErrorCode {
    pub(crate) fn as_api_code(self) -> &'static str {
        match self {
            Self::AuthFailed => remote_sftp_codes::REMOTE_AUTH_FAILED,
            Self::PathNotFound => remote_sftp_codes::REMOTE_PATH_NOT_FOUND,
            Self::PermissionDenied => remote_sftp_codes::REMOTE_PERMISSION_DENIED,
            Self::NetworkDisconnected => remote_sftp_codes::REMOTE_NETWORK_DISCONNECTED,
            Self::Protocol => remote_sftp_codes::REMOTE_ERROR,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransferJobError {
    Cancelled,
    Timeout(String),
    Io(String),
    Remote {
        code: RemoteTransferErrorCode,
        message: String,
    },
    Message(String),
}

impl TransferJobError {
    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    pub(crate) fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout(message.into())
    }

    pub(crate) fn io(message: impl Into<String>) -> Self {
        Self::Io(message.into())
    }

    pub(crate) fn remote(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::Remote {
            code: classify_remote_transfer_error_code(message.as_str()),
            message,
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl Display for TransferJobError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "传输已取消"),
            Self::Timeout(message) => write!(f, "{message}"),
            Self::Io(message) => write!(f, "{message}"),
            Self::Remote { message, .. } => write!(f, "{message}"),
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

fn classify_remote_transfer_error_code(message: &str) -> RemoteTransferErrorCode {
    let normalized = message.to_lowercase();
    if normalized.contains("authentication")
        || normalized.contains("auth fail")
        || normalized.contains("ssh 认证失败")
        || normalized.contains("permission denied (publickey")
        || normalized.contains("permission denied, please try again")
    {
        return RemoteTransferErrorCode::AuthFailed;
    }
    if normalized.contains("no such file")
        || normalized.contains("not found")
        || normalized.contains("路径不存在")
    {
        return RemoteTransferErrorCode::PathNotFound;
    }
    if normalized.contains("permission denied") || normalized.contains("权限不足") {
        return RemoteTransferErrorCode::PermissionDenied;
    }
    if normalized.contains("connection reset")
        || normalized.contains("broken pipe")
        || normalized.contains("connection closed")
        || normalized.contains("connection timed out")
        || normalized.contains("timed out")
        || normalized.contains("network is unreachable")
        || normalized.contains("no route to host")
        || normalized.contains("网络中断")
    {
        return RemoteTransferErrorCode::NetworkDisconnected;
    }
    RemoteTransferErrorCode::Protocol
}
