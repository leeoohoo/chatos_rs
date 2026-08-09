// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::core::remote_connection_error_codes::remote_sftp_codes;

use super::super::error_support::extract_second_factor_required_prompt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteSftpApiError {
    code: &'static str,
    error: String,
    challenge_prompt: Option<String>,
}

impl RemoteSftpApiError {
    pub(super) fn bad_request_with_code(code: &'static str, error: impl Into<String>) -> Self {
        Self {
            code,
            error: error.into(),
            challenge_prompt: None,
        }
    }

    pub(super) fn remote_error(error: impl Into<String>) -> Self {
        let error = error.into();
        if let Some(prompt) = extract_second_factor_required_prompt(error.as_str()) {
            return Self {
                code: remote_sftp_codes::SECOND_FACTOR_REQUIRED,
                error: "需要二次验证".to_string(),
                challenge_prompt: Some(prompt),
            };
        }
        Self::bad_request_with_code(remote_sftp_codes::REMOTE_ERROR, error)
    }

    pub(super) fn into_response(self) -> (StatusCode, Json<Value>) {
        let mut payload = serde_json::json!({ "error": self.error, "code": self.code });
        if let Some(prompt) = self.challenge_prompt {
            payload["challenge_prompt"] = serde_json::json!(prompt);
        }
        (StatusCode::BAD_REQUEST, Json(payload))
    }
}
