// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::{Digest, Sha512};

use crate::auth::{
    bearer_token_from_headers, verify_request_via_user_service, DeviceProofVerificationRequest,
};
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

    #[cfg(any(test, feature = "test-support"))]
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

pub(super) async fn require_internal_auth(
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
    let proof = device_proof_request(&request, "local");
    let user = verify_request_via_user_service(
        &state.config,
        &state.user_service_http,
        token.as_str(),
        &proof,
    )
    .await
    .map_err(ApiError::unauthorized)?;
    if user.is_wechat_companion() {
        verify_and_restore_device_bound_body(&mut request, proof.body_sha512.as_str()).await?;
    }
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

fn enforce_client_scope(
    user: &crate::models::CurrentUser,
    method: &Method,
    path: &str,
) -> Result<(), ApiError> {
    if !user.is_wechat_companion() {
        return Ok(());
    }
    if companion_request_allowed(method, path) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "WeChat Companion session is not allowed to access this Local Connector endpoint",
        ))
    }
}

fn companion_request_allowed(method: &Method, path: &str) -> bool {
    if method == Method::GET && path == "/api/local-connectors/companion/devices" {
        return true;
    }
    let Some(suffix) = path.strip_prefix("/api/local-connectors/companion/devices/") else {
        return false;
    };
    let segments = suffix.split('/').collect::<Vec<_>>();
    matches!((method, segments.as_slice()),
        (&Method::GET, [device_id, "resources"]) if !device_id.is_empty()
    ) || matches!((method, segments.as_slice()),
        (&Method::POST, [device_id, "resources", "resolve"]) if !device_id.is_empty()
    ) || matches!((method, segments.as_slice()),
        (&Method::GET, [device_id, "approvals"]) if !device_id.is_empty()
    ) || matches!((method, segments.as_slice()),
        (&Method::POST, [device_id, "approvals", approval_id, "resolve"])
            if !device_id.is_empty() && !approval_id.is_empty()
    )
}

pub(super) async fn require_public_auth(
    State(state): State<AuthState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    let token = bearer_token_from_request(&request).map_err(ApiError::unauthorized)?;
    let proof = device_proof_request(&request, "local");
    let user = verify_request_via_user_service(
        &state.config,
        &state.user_service_http,
        token.as_str(),
        &proof,
    )
    .await
    .map_err(ApiError::unauthorized)?;
    if user.is_wechat_companion() {
        verify_and_restore_device_bound_body(&mut request, proof.body_sha512.as_str()).await?;
    }
    enforce_client_scope(&user, request.method(), request.uri().path())?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

fn device_proof_request(
    request: &Request<axum::body::Body>,
    surface: &str,
) -> DeviceProofVerificationRequest {
    let header = |name: &'static str| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let internal_path = request.uri().path();
    let logical_path = internal_path
        .strip_prefix("/api/local-connectors")
        .unwrap_or(internal_path);
    let mut target = format!("/api/{surface}{logical_path}");
    if let Some(query) = request.uri().query().filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }
    DeviceProofVerificationRequest {
        surface: surface.to_string(),
        method: request.method().as_str().to_string(),
        target,
        body_sha512: header("x-chatos-device-body-sha512"),
        client_session_id: header("x-chatos-device-session-id"),
        device_id: header("x-chatos-device-id"),
        timestamp: header("x-chatos-device-timestamp").parse().unwrap_or_default(),
        nonce: header("x-chatos-device-nonce"),
        signature_algorithm: header("x-chatos-device-signature-alg"),
        signature: header("x-chatos-device-signature"),
    }
}

async fn verify_and_restore_device_bound_body(
    request: &mut Request<axum::body::Body>,
    expected_hash: &str,
) -> Result<(), ApiError> {
    const LIMIT: usize = 4 * 1024 * 1024;
    let body = std::mem::replace(request.body_mut(), axum::body::Body::empty());
    let bytes = axum::body::to_bytes(body, LIMIT)
        .await
        .map_err(|_| ApiError::unauthorized("device-bound request body is too large"))?;
    let actual = URL_SAFE_NO_PAD.encode(Sha512::digest(bytes.as_ref()));
    *request.body_mut() = axum::body::Body::from(bytes);
    if actual != expected_hash {
        return Err(ApiError::unauthorized(
            "device proof body digest does not match the request",
        ));
    }
    Ok(())
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
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::*;

    fn request(uri: &str) -> Request<axum::body::Body> {
        Request::builder()
            .uri(uri)
            .body(axum::body::Body::empty())
            .expect("test request should be valid")
    }

    fn companion_user() -> crate::models::CurrentUser {
        crate::models::CurrentUser {
            principal_type: "human_user".to_string(),
            token_jti: Some("companion-session-1".to_string()),
            user_id: "user-1".to_string(),
            username: Some("user".to_string()),
            display_name: Some("User".to_string()),
            role: "user".to_string(),
            owner_user_id: None,
            scopes: vec!["wechat_companion".to_string()],
        }
    }

    #[test]
    fn companion_scope_only_allows_sanitized_device_summary() {
        let user = companion_user();
        assert!(enforce_client_scope(
            &user,
            &Method::GET,
            "/api/local-connectors/companion/devices",
        )
        .is_ok());
        assert!(enforce_client_scope(
            &user,
            &Method::GET,
            "/api/local-connectors/companion/devices/device-1/resources",
        )
        .is_ok());
        assert!(enforce_client_scope(
            &user,
            &Method::POST,
            "/api/local-connectors/companion/devices/device-1/resources/resolve",
        )
        .is_ok());
        assert!(enforce_client_scope(
            &user,
            &Method::GET,
            "/api/local-connectors/companion/devices/device-1/approvals",
        )
        .is_ok());
        assert!(enforce_client_scope(
            &user,
            &Method::POST,
            "/api/local-connectors/companion/devices/device-1/approvals/approval-1/resolve",
        )
        .is_ok());
        for (method, path) in [
            (Method::GET, "/api/local-connectors/devices"),
            (Method::POST, "/api/local-connectors/devices"),
            (Method::GET, "/api/local-connectors/workspaces"),
            (Method::POST, "/api/local-connectors/relay/device-1/mcp"),
            (
                Method::GET,
                "/api/local-connectors/companion/devices/device-1/resources/extra",
            ),
            (
                Method::POST,
                "/api/local-connectors/companion/devices/device-1/resources",
            ),
            (
                Method::GET,
                "/api/local-connectors/companion/devices/device-1/approvals/approval-1",
            ),
            (
                Method::POST,
                "/api/local-connectors/companion/devices/device-1/approvals//resolve",
            ),
        ] {
            assert!(
                enforce_client_scope(&user, &method, path).is_err(),
                "path={path}"
            );
        }
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

    #[tokio::test]
    async fn public_listener_rejects_internal_service_identity() {
        let secret = "a-long-public-boundary-test-secret";
        let config = crate::config::AppConfig::for_plugin_artifact_relay_test(secret);
        let auth_state = AuthState::for_test(config).expect("test auth state");
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            "chatos-backend",
            super::super::internal_auth::TOKEN_AUDIENCE,
            "relay.mcp",
            60,
        )
        .expect("internal token");
        let app = Router::new()
            .route(
                "/api/local-connectors/relay/device-1/mcp",
                get(|| async { "ok" }),
            )
            .route_layer(middleware::from_fn_with_state(
                auth_state,
                require_public_auth,
            ));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/local-connectors/relay/device-1/mcp")
                    .header("x-local-connector-caller", "chatos-backend")
                    .header("x-local-connector-internal-token", token)
                    .body(axum::body::Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
