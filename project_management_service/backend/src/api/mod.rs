mod router;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::models::ErrorResponse;

pub use router::build_router;

const PROJECT_MANAGEMENT_MCP_SKILL_ZH_CN: &str =
    include_str!("../../../PROJECT_MANAGEMENT_MCP_SKILL.zh-CN.md");
const PROJECT_MANAGEMENT_MCP_SKILL_EN_US: &str =
    include_str!("../../../PROJECT_MANAGEMENT_MCP_SKILL.en-US.md");

#[derive(Debug, Serialize)]
pub(in crate::api) struct ProjectManagementSkillResponse {
    name: &'static str,
    locale: &'static str,
    content: &'static str,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
                details: None,
            }),
        )
            .into_response()
    }
}
