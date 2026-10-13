// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Browser-only session recovery. Ordinary APIs still require explicit Bearer auth.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;

use crate::auth::{
    decode_user_service_token, issue_user_token_with_scopes, AuthClaims, CurrentPrincipal,
};
use crate::models::{LoginRequest, LoginResponse, PRINCIPAL_TYPE_HUMAN_USER};
use crate::state::AppState;

use super::{auth::authenticate_password_user, forbidden, internal_error, unauthorized};

const COOKIE_NAME: &str = "__Secure-chatos_admin_session";
const COOKIE_PATH: &str = "/api/admin/user-service/auth/admin";
const SESSION_SCOPE: &str = "admin_browser_session";
const MAX_SESSION_SECONDS: i64 = 8 * 60 * 60;

// An explicit same-origin POST prevents login CSRF and cross-origin token disclosure.
fn require_browser_request(
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let allowed_origin = origin.is_some_and(|origin| {
        origin == "https://admin.jgoool.com"
            || matches!(origin, "http://localhost:39200" | "http://127.0.0.1:39200")
    });
    let fetch_site = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok());
    if !allowed_origin
        || headers
            .get("x-chatos-admin-session")
            .and_then(|value| value.to_str().ok())
            != Some("1")
        || fetch_site.is_some_and(|site| site != "same-origin")
    {
        return Err(forbidden(
            "same-origin administrator session request required",
        ));
    }
    Ok(())
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let mut tokens = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|value| value.trim().split_once('='))
        .filter(|(name, _)| *name == COOKIE_NAME)
        .map(|(_, value)| value.to_string());
    let token = tokens.next()?;
    // Reject ambiguous cookies rather than choosing an attacker-controlled duplicate.
    (!token.is_empty() && tokens.next().is_none()).then_some(token)
}

fn admin_claims(claims: &AuthClaims) -> bool {
    claims.principal_type == PRINCIPAL_TYPE_HUMAN_USER
        && claims.scopes.iter().any(|scope| scope == SESSION_SCOPE)
}

fn is_admin(principal: &CurrentPrincipal) -> bool {
    principal.principal_type == PRINCIPAL_TYPE_HUMAN_USER
        && matches!(principal.role.as_deref(), Some("super_admin" | "admin"))
}

fn session_response(body: impl IntoResponse, token: &str, max_age: i64) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={token}; Path={COOKIE_PATH}; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}"
    )).expect("issued JWT is a valid cookie value"));
    response
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<LoginRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    require_browser_request(&headers)?;
    let username = crate::auth::normalize_username(&input.username).map_err(super::bad_request)?;
    let source = crate::login_throttle::request_source(&headers, addr);
    let user = authenticate_password_user(
        &state,
        &username,
        &input.password,
        &source,
        Utc::now().timestamp(),
    )
    .await?;
    if !matches!(user.role.as_str(), "super_admin" | "admin") {
        return Err(forbidden("administrator permission required"));
    }
    let _ = crate::integrations::ensure_harness_user_public_register_on_login(&state, &user).await;
    let issued = issue_user_token_with_scopes(
        &state.config,
        &user,
        state
            .config
            .user_access_ttl_seconds
            .clamp(60, MAX_SESSION_SECONDS),
        vec!["user_service".to_string(), SESSION_SCOPE.to_string()],
    )
    .map_err(internal_error)?;
    let principal = CurrentPrincipal::from(
        decode_user_service_token(&issued.token, &state.config).map_err(internal_error)?,
    );
    let body = Json(LoginResponse {
        token: issued.token.clone(),
        user: principal.auth_user(),
    });
    Ok(session_response(
        body,
        &issued.token,
        issued.expires_at_unix - Utc::now().timestamp(),
    ))
}

pub async fn restore(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    require_browser_request(&headers)?;
    let token = cookie_token(&headers).ok_or_else(|| unauthorized("no administrator session"))?;
    let claims = decode_user_service_token(&token, &state.config)
        .map_err(|_| unauthorized("invalid or expired administrator session"))?;
    if !admin_claims(&claims) || claims.exp as i64 <= Utc::now().timestamp() {
        return Err(unauthorized("invalid administrator session"));
    }
    if state
        .store
        .is_token_revoked(&claims.jti)
        .await
        .map_err(internal_error)?
    {
        return Err(unauthorized("administrator session has been revoked"));
    }
    let mut principal = CurrentPrincipal::from(claims);
    // Reload enabled state, current role and credential version, not just JWT claims.
    super::refresh_principal_identity(&state, &mut principal).await?;
    if !is_admin(&principal) {
        return Err(forbidden("administrator permission required"));
    }
    let max_age = principal.exp as i64 - Utc::now().timestamp();
    let body = Json(LoginResponse {
        token: token.clone(),
        user: principal.auth_user(),
    });
    Ok(session_response(body, &token, max_age))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    require_browser_request(&headers)?;
    if let Some(claims) = cookie_token(&headers)
        .and_then(|token| decode_user_service_token(&token, &state.config).ok())
    {
        if admin_claims(&claims) {
            state
                .store
                .revoke_token(&claims.jti, &claims.sub, claims.exp as i64)
                .await
                .map_err(internal_error)?;
        }
    }
    // Expired/missing cookies can also be cleared, without requiring a valid Bearer token.
    Ok(session_response(StatusCode::NO_CONTENT, "", 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://admin.jgoool.com"),
        );
        headers.insert("x-chatos-admin-session", HeaderValue::from_static("1"));
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        headers
    }

    #[test]
    fn rejects_cross_origin_and_missing_csrf_headers() {
        let mut headers = browser_headers();
        assert!(require_browser_request(&headers).is_ok());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(require_browser_request(&headers).is_err());
        headers = browser_headers();
        headers.remove("x-chatos-admin-session");
        assert!(require_browser_request(&headers).is_err());
        headers = browser_headers();
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-site"));
        assert!(require_browser_request(&headers).is_err());
    }

    #[test]
    fn cookie_is_secure_host_only_path_scoped_and_not_cacheable() {
        let response = session_response(StatusCode::OK, "test-token", 3600);
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(cookie.contains("HttpOnly; Secure; SameSite=Strict"));
        assert!(cookie.contains(&format!("Path={COOKIE_PATH}")));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(!cookie.contains("Domain="));
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let cleared = session_response(StatusCode::NO_CONTENT, "", 0);
        assert!(cleared.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0"));
    }

    #[test]
    fn only_the_dedicated_unambiguous_cookie_is_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("other=value"));
        assert!(cookie_token(&headers).is_none());
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=value; __Secure-chatos_admin_session=jwt"),
        );
        assert_eq!(cookie_token(&headers).as_deref(), Some("jwt"));
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Secure-chatos_admin_session=another"),
        );
        assert!(cookie_token(&headers).is_none());
    }
}
