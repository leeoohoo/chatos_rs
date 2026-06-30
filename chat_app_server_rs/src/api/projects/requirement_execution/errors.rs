use axum::http::StatusCode;

#[derive(Debug)]
pub(in crate::api::projects) struct HandlerError {
    pub(in crate::api::projects) status: StatusCode,
    pub(in crate::api::projects) error: String,
    pub(in crate::api::projects) detail: Option<String>,
}

impl HandlerError {
    pub(in crate::api::projects) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: message.into(),
            detail: None,
        }
    }

    pub(in crate::api::projects) fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            error: message.into(),
            detail: None,
        }
    }

    pub(in crate::api::projects) fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            error: message.into(),
            detail: None,
        }
    }

    pub(in crate::api::projects) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error: message.into(),
            detail: None,
        }
    }

    pub(in crate::api::projects) fn internal(
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: message.into(),
            detail: Some(detail.into()),
        }
    }

    pub(in crate::api::projects) fn bad_gateway(
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            error: message.into(),
            detail: Some(detail.into()),
        }
    }
}
