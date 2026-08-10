// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;

use super::errors::RemoteSftpApiError;
use crate::core::remote_connection_error_codes::remote_sftp_codes;

#[test]
fn maps_second_factor_required_to_structured_payload() {
    let (status, body) = RemoteSftpApiError::remote_error(
        "__CHATOS_SECOND_FACTOR_REQUIRED__:SMS verification code".to_string(),
    )
    .into_response();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.0["code"], remote_sftp_codes::SECOND_FACTOR_REQUIRED);
    assert_eq!(body.0["challenge_prompt"], "SMS verification code");
}

#[test]
fn rejects_invalid_directory_name() {
    let error = super::support::validate_mkdir_name("../escape").unwrap_err();
    let (_, body) = error.into_response();
    assert_eq!(body.0["code"], remote_sftp_codes::INVALID_DIRECTORY_NAME);
}
