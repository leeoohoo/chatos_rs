use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use axum::extract::FromRequestParts;
use axum::http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::config::Config;

const DEFAULT_CHAT_APP_AUTH_SECRET: &str = "dev-only-change-me-please";
const DEFAULT_LEGACY_COMPAT_AUTH_SECRET: &str = "legacy_compat_dev_change_me";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthHeaderError {
    MissingAuthorization,
    InvalidAuthorization,
    InvalidOrExpiredToken,
}

#[derive(Debug)]
pub enum AuthResolveError {
    InvalidOrExpiredToken,
    ConfigUnavailable(String),
}

impl AuthHeaderError {
    fn message(self) -> &'static str {
        match self {
            Self::MissingAuthorization => "缺少 Authorization",
            Self::InvalidAuthorization => "Authorization 格式错误",
            Self::InvalidOrExpiredToken => "登录状态无效或已过期",
        }
    }

    pub fn into_response(self) -> (StatusCode, Json<serde_json::Value>) {
        unauthorized(self.message())
    }
}

impl AuthResolveError {
    pub fn into_response(self) -> (StatusCode, Json<serde_json::Value>) {
        match self {
            Self::InvalidOrExpiredToken => AuthHeaderError::InvalidOrExpiredToken.into_response(),
            Self::ConfigUnavailable(detail) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "认证配置未初始化",
                    "detail": detail
                })),
            ),
        }
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(auth_user) = parts.extensions.get::<AuthUser>() {
            return Ok(auth_user.clone());
        }
        let access_token =
            access_token_from_headers(&parts.headers).map_err(AuthHeaderError::into_response)?;
        resolve_auth_user_from_token(access_token.as_str()).map_err(AuthResolveError::into_response)
    }
}

pub fn access_token_from_headers(headers: &HeaderMap) -> Result<String, AuthHeaderError> {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return Err(AuthHeaderError::MissingAuthorization);
    };
    let Ok(raw) = value.to_str() else {
        return Err(AuthHeaderError::InvalidAuthorization);
    };
    let Some(token) = raw.strip_prefix("Bearer ").map(str::trim) else {
        return Err(AuthHeaderError::InvalidAuthorization);
    };
    access_token_from_raw(token)
}

pub fn access_token_from_raw(token: &str) -> Result<String, AuthHeaderError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(AuthHeaderError::InvalidOrExpiredToken);
    }
    Ok(trimmed.to_string())
}

pub fn resolve_auth_user_from_token(access_token: &str) -> Result<AuthUser, AuthResolveError> {
    let cfg = Config::try_get().map_err(AuthResolveError::ConfigUnavailable)?;
    let parsed = auth_token_secrets(cfg)
        .into_iter()
        .find_map(|secret| parse_compat_auth_token(access_token, secret));
    let (user_id, role, _) = parsed.ok_or(AuthResolveError::InvalidOrExpiredToken)?;
    Ok(AuthUser { user_id, role })
}

pub fn build_auth_token(user_id: &str, role: &str) -> Result<String, String> {
    let cfg = Config::try_get()?;
    let exp =
        (chrono::Utc::now() + chrono::Duration::seconds(cfg.auth_access_token_ttl_seconds.max(60)))
            .timestamp();
    let payload = format!("{}|{}|{}", user_id, role, exp);
    let sig = sign_compat_auth_payload(payload.as_str(), auth_token_signing_secret(cfg));
    Ok(URL_SAFE_NO_PAD.encode(format!("{}|{}", payload, sig)))
}

fn parse_compat_auth_token(token: &str, secret: &str) -> Option<(String, String, i64)> {
    let decoded = URL_SAFE_NO_PAD.decode(token.as_bytes()).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let mut parts = decoded.split('|');
    let user_id = parts.next()?.to_string();
    let role = parts.next()?.to_string();
    let exp = parts.next()?.parse::<i64>().ok()?;
    let sig = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }

    let payload = format!("{}|{}|{}", user_id, role, exp);
    if sign_compat_auth_payload(payload.as_str(), secret) != sig {
        return None;
    }
    if chrono::Utc::now().timestamp() > exp {
        return None;
    }
    Some((user_id, role, exp))
}

fn sign_compat_auth_payload(payload: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    hasher.update(b"|");
    hasher.update(secret.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn auth_token_secrets<'a>(cfg: &'a Config) -> Vec<&'a str> {
    let mut secrets = vec![cfg.auth_jwt_secret.as_str()];
    if cfg.auth_jwt_secret != DEFAULT_LEGACY_COMPAT_AUTH_SECRET {
        secrets.push(DEFAULT_LEGACY_COMPAT_AUTH_SECRET);
    }
    secrets
}

fn auth_token_signing_secret(cfg: &Config) -> &str {
    if cfg.auth_jwt_secret == DEFAULT_CHAT_APP_AUTH_SECRET {
        DEFAULT_LEGACY_COMPAT_AUTH_SECRET
    } else {
        cfg.auth_jwt_secret.as_str()
    }
}

fn unauthorized(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": message
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_compat_auth_token, sign_compat_auth_payload};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    fn build_compat_auth_token(user_id: &str, role: &str, secret: &str, exp: i64) -> String {
        let payload = format!("{}|{}|{}", user_id, role, exp);
        let sig = sign_compat_auth_payload(payload.as_str(), secret);
        URL_SAFE_NO_PAD.encode(format!("{}|{}", payload, sig))
    }

    #[test]
    fn parses_valid_compat_token() {
        let exp = chrono::Utc::now().timestamp() + 3600;
        let token = build_compat_auth_token("alice", "admin", "secret-1", exp);
        let parsed = parse_compat_auth_token(token.as_str(), "secret-1").expect("parse token");
        assert_eq!(parsed.0, "alice");
        assert_eq!(parsed.1, "admin");
        assert_eq!(parsed.2, exp);
    }

    #[test]
    fn rejects_token_signed_with_other_secret() {
        let exp = chrono::Utc::now().timestamp() + 3600;
        let token = build_compat_auth_token("alice", "admin", "secret-1", exp);
        assert!(parse_compat_auth_token(token.as_str(), "secret-2").is_none());
    }
}
