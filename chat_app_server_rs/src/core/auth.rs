use axum::extract::FromRequestParts;
use axum::http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::config::Config;

const DEFAULT_LEGACY_COMPAT_AUTH_SECRET: &str = "legacy_compat_dev_change_me";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UserServiceAuthClaims {
    iss: String,
    aud: String,
    exp: usize,
    principal_type: String,
    user_id: Option<String>,
    role: Option<String>,
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
    if let Some(user) = parse_user_service_auth_token(access_token, cfg) {
        return Ok(user);
    }
    let parsed = auth_token_secrets(cfg)
        .into_iter()
        .find_map(|secret| parse_compat_auth_token(access_token, secret));
    let (user_id, role, _) = parsed.ok_or(AuthResolveError::InvalidOrExpiredToken)?;
    Ok(AuthUser { user_id, role })
}

pub fn build_auth_token(user_id: &str, role: &str) -> Result<String, String> {
    let cfg = Config::try_get()?;
    let exp = (chrono::Utc::now()
        + chrono::Duration::seconds(cfg.auth_access_token_ttl_seconds.max(60)))
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

fn parse_user_service_auth_token(token: &str, cfg: &Config) -> Option<AuthUser> {
    let secret = cfg.user_service_jwt_secret.as_deref()?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[cfg.user_service_jwt_issuer.as_str()]);
    validation.set_audience(&[cfg.user_service_user_audience.as_str()]);
    let claims = decode::<UserServiceAuthClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()?
    .claims;
    if claims.iss.trim().is_empty()
        || claims.aud.trim().is_empty()
        || claims.exp == 0
        || claims.principal_type != "human_user"
    {
        return None;
    }
    let user_id = claims.user_id?.trim().to_string();
    let role = claims.role?.trim().to_string();
    if user_id.is_empty() || role.is_empty() {
        return None;
    }
    Some(AuthUser { user_id, role })
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
    if let Some(secret) = cfg.auth_compat_secret.as_deref() {
        if secret != cfg.auth_jwt_secret {
            secrets.push(secret);
        }
    }
    if cfg.auth_jwt_secret != DEFAULT_LEGACY_COMPAT_AUTH_SECRET {
        secrets.push(DEFAULT_LEGACY_COMPAT_AUTH_SECRET);
    }
    secrets
}

fn auth_token_signing_secret(cfg: &Config) -> &str {
    cfg.auth_jwt_secret.as_str()
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
    use super::{
        auth_token_secrets, parse_compat_auth_token, parse_user_service_auth_token,
        sign_compat_auth_payload, UserServiceAuthClaims, DEFAULT_LEGACY_COMPAT_AUTH_SECRET,
    };
    use crate::config::Config;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn build_compat_auth_token(user_id: &str, role: &str, secret: &str, exp: i64) -> String {
        let payload = format!("{}|{}|{}", user_id, role, exp);
        let sig = sign_compat_auth_payload(payload.as_str(), secret);
        URL_SAFE_NO_PAD.encode(format!("{}|{}", payload, sig))
    }

    fn build_test_config() -> Config {
        Config {
            openai_api_key: String::new(),
            openai_base_url: "https://api.openai.com/v1".to_string(),
            port: 3997,
            node_env: "development".to_string(),
            host: "0.0.0.0".to_string(),
            log_level: "info".to_string(),
            log_max_files: "7d".to_string(),
            cors_origins: vec!["*".to_string()],
            summary_enabled: true,
            summary_message_limit: 40,
            summary_max_context_tokens: 6000,
            summary_keep_last_n: 6,
            summary_target_tokens: 700,
            summary_merge_target_tokens: 700,
            summary_temperature: 0.2,
            summary_cooldown_seconds: 60,
            dynamic_summary_enabled: true,
            summary_bisect_enabled: true,
            summary_bisect_max_depth: 6,
            summary_bisect_min_messages: 4,
            summary_retry_on_context_overflow: true,
            auth_jwt_secret: "primary-secret".to_string(),
            auth_compat_secret: Some("compat-secret".to_string()),
            auth_access_token_ttl_seconds: 3600,
            user_service_base_url: Some("http://127.0.0.1:39190".to_string()),
            user_service_request_timeout_ms: 5000,
            user_service_jwt_secret: Some("user-service-secret".to_string()),
            user_service_jwt_issuer: "user_service".to_string(),
            user_service_user_audience: "user_service".to_string(),
            task_runner_base_url: "http://127.0.0.1:39090".to_string(),
            memory_engine_base_url: "http://127.0.0.1:7081/api/memory-engine/v1".to_string(),
            memory_engine_request_timeout_ms: 5000,
            memory_engine_active_summary_trigger_timeout_ms: 5000,
            memory_engine_active_summary_poll_interval_ms: 10_000,
            memory_engine_active_summary_poll_timeout_ms: 120_000,
            task_runner_callback_secret: None,
        }
    }

    fn build_user_service_auth_token(claims: &UserServiceAuthClaims, secret: &str) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("encode user_service auth token")
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

    #[test]
    fn parses_valid_user_service_human_user_token() {
        let cfg = build_test_config();
        let token = build_user_service_auth_token(
            &UserServiceAuthClaims {
                iss: "user_service".to_string(),
                aud: "user_service".to_string(),
                exp: (chrono::Utc::now().timestamp() + 3600) as usize,
                principal_type: "human_user".to_string(),
                user_id: Some("user-123".to_string()),
                role: Some("user".to_string()),
            },
            cfg.user_service_jwt_secret
                .as_deref()
                .expect("missing secret"),
        );

        let user = parse_user_service_auth_token(token.as_str(), &cfg).expect("parse token");
        assert_eq!(user.user_id, "user-123");
        assert_eq!(user.role, "user");
    }

    #[test]
    fn rejects_user_service_agent_account_token_for_human_auth() {
        let cfg = build_test_config();
        let token = build_user_service_auth_token(
            &UserServiceAuthClaims {
                iss: "user_service".to_string(),
                aud: "user_service".to_string(),
                exp: (chrono::Utc::now().timestamp() + 3600) as usize,
                principal_type: "agent_account".to_string(),
                user_id: Some("user-123".to_string()),
                role: Some("user".to_string()),
            },
            cfg.user_service_jwt_secret
                .as_deref()
                .expect("missing secret"),
        );

        assert!(parse_user_service_auth_token(token.as_str(), &cfg).is_none());
    }

    #[test]
    fn auth_token_secrets_include_explicit_compat_secret() {
        let cfg = build_test_config();

        let secrets = auth_token_secrets(&cfg);
        assert_eq!(secrets[0], "primary-secret");
        assert!(secrets.contains(&"compat-secret"));
        assert!(secrets.contains(&DEFAULT_LEGACY_COMPAT_AUTH_SECRET));
    }
}
