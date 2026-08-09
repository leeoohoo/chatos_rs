// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::HeaderMap;

use crate::core::remote_connection_error_codes::remote_sftp_codes;
use crate::core::validation::normalize_non_empty;

use super::errors::RemoteSftpApiError;

const REMOTE_VERIFICATION_CODE_HEADER: &str = "x-remote-verification-code";

pub(super) fn verification_code_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(REMOTE_VERIFICATION_CODE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn require_non_empty_field(
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

pub(super) fn validate_mkdir_name(name: &str) -> Result<(), RemoteSftpApiError> {
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(RemoteSftpApiError::bad_request_with_code(
            remote_sftp_codes::INVALID_DIRECTORY_NAME,
            "目录名不合法",
        ));
    }
    Ok(())
}
