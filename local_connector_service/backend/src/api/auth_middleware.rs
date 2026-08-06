// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::auth::{bearer_token_from_headers, verify_token_via_user_service};
use crate::models::ErrorResponse;
use crate::state::AppState;

use super::internal_auth::internal_service_auth_from_request;

#[derive(Clone)]
pub(super) struct AuthState {
    config: crate::config::AppConfig,
    user_service_http: reqwest::Client,
}

impl AuthState {
    pub(super) fn from_app_state(state: &AppState) -> Self {
        Self {
            config: state.config.clone(),
            user_service_http: state.user_service_http().clone(),
        }
    }

    #[cfg(feature = "test-support")]
    pub(super) fn for_test(config: crate::config::AppConfig) -> Result<Self, String> {
        let user_service_http = reqwest::Client::builder()
            .timeout(config.user_service_request_timeout)
            .build()
            .map_err(|error| format!("build test auth client failed: {error}"))?;
        Ok(Self {
            config,
            user_service_http,
        })
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    code: Option<String>,
}

impl ApiError {
    pub fn message(&self) -> &str {
        self.message.as_str()
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            code: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            code: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            code: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            code: None,
        }
    }

    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: message.into(),
            code: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            code: None,
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
            code: None,
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
            code: None,
        }
    }

    pub fn gateway_timeout(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: message.into(),
            code: None,
        }
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            message: message.into(),
            code: None,
        }
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
                code: self.code,
            }),
        )
            .into_response()
    }
}

pub(super) async fn require_auth(
    State(state): State<AuthState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    if let Some((user, identity)) = internal_service_auth_from_request(
        &state.config,
        request.headers(),
        request.method(),
        request.uri().path(),
    )? {
        let method = request.method().to_string();
        let resource_path = request.uri().path().to_string();
        request.extensions_mut().insert(user);
        request.extensions_mut().insert(identity.clone());
        let response = next.run(request).await;
        let event = chatos_service_runtime::InternalResourceAccessAudit {
            caller_service: identity.caller_service,
            audience_service: super::internal_auth::TOKEN_AUDIENCE.to_string(),
            scope: identity.scope,
            trace_id: identity.trace_id,
            represented_user_id: Some(identity.owner_user_id),
            tenant_id: None,
            project_id: None,
            resource_type: "local_connector_internal_route".to_string(),
            resource_id: resource_path,
            resource_name: None,
            action: method,
            outcome: response.status().as_u16().to_string(),
        };
        if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
            tracing::error!(
                error = error.as_str(),
                "record Local Connector internal access audit failed"
            );
        }
        return Ok(response);
    }
    let token = bearer_token_from_request(&request).map_err(ApiError::unauthorized)?;
    let user =
        verify_token_via_user_service(&state.config, &state.user_service_http, token.as_str())
            .await
            .map_err(ApiError::unauthorized)?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

fn bearer_token_from_request(request: &Request<axum::body::Body>) -> Result<String, String> {
    if let Ok(token) = bearer_token_from_headers(request.headers()) {
        return Ok(token.to_string());
    }

    let query = request.uri().query();
    if !has_legacy_query_token(query) {
        return Err("缺少登录令牌".to_string());
    }

    Err("URL query access tokens are not supported; use Authorization header".to_string())
}

fn has_legacy_query_token(query: Option<&str>) -> bool {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .any(|pair| {
            let key = pair.split_once('=').map_or(pair, |(key, _)| key);
            key == "access_token" || key == "token"
        })
}

#[cfg(test)]
mod tests {
    use axum::http::header::AUTHORIZATION;

    use super::*;

    fn request(uri: &str) -> Request<axum::body::Body> {
        Request::builder()
            .uri(uri)
            .body(axum::body::Body::empty())
            .expect("test request should be valid")
    }

    #[test]
    fn header_token_is_preferred_over_device_query_token() {
        let mut request =
            request("/api/local-connectors/devices/device-1/connect?access_token=query-token");
        request
            .headers_mut()
            .insert(AUTHORIZATION, "Bearer header-token".parse().unwrap());

        let token = bearer_token_from_request(&request).expect("header token should pass");

        assert_eq!(token, "header-token");
    }

    #[test]
    fn non_device_query_token_is_rejected() {
        let request = request("/api/local-connectors/devices?access_token=query-token");

        let error = bearer_token_from_request(&request).expect_err("query token must fail");

        assert_eq!(
            error,
            "URL query access tokens are not supported; use Authorization header"
        );
    }

    #[test]
    fn device_query_token_is_always_rejected() {
        let request =
            request("/api/local-connectors/devices/device-1/connect?access_token=query-token");

        let error = bearer_token_from_request(&request).expect_err("query token must fail");

        assert_eq!(
            error,
            "URL query access tokens are not supported; use Authorization header"
        );
    }
}
