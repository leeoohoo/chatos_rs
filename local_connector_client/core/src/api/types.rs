// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AuthUserState;

#[derive(Debug, Deserialize)]
pub(super) struct LoginResponse {
    pub(super) token: String,
    pub(super) user: AuthUserState,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalAuthRequest {
    pub(super) cloud_base_url: String,
    pub(super) user_service_base_url: Option<String>,
    pub(super) username: String,
    pub(super) password: String,
    pub(super) display_name: Option<String>,
    pub(super) device_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AddWorkspaceRequest {
    pub(super) path: String,
    pub(super) alias: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FsListQuery {
    pub(super) path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CommandHistoryQuery {
    pub(super) limit: Option<usize>,
    pub(super) source: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct FsListResponse {
    pub(super) path: String,
    pub(super) parent: Option<String>,
    pub(super) entries: Vec<FsEntry>,
}

#[derive(Debug, Serialize)]
pub(super) struct FsEntry {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) is_dir: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct ToggleSandboxRequest {
    pub(super) enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct InitializeImageRequest {
    pub(super) features: Vec<String>,
    pub(super) custom_build_script: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalTerminalExecRequest {
    pub(super) workspace_id: String,
    pub(super) command: String,
    pub(super) args: Option<Vec<String>>,
    pub(super) cwd: Option<String>,
    pub(super) timeout_ms: Option<u64>,
}

#[derive(Debug)]
pub(super) struct LocalApiError {
    status: axum::http::StatusCode,
    message: String,
}

impl LocalApiError {
    pub(super) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    pub(super) fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for LocalApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<anyhow::Error> for LocalApiError {
    fn from(value: anyhow::Error) -> Self {
        Self::internal(value.to_string())
    }
}
