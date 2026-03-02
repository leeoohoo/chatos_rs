use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode};
use axum::Json;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: String,
    pub email: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub email: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthHeaderError {
    MissingAuthorization,
    InvalidAuthorization,
    InvalidOrExpiredToken,
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
        auth_user_from_headers(&parts.headers).map_err(AuthHeaderError::into_response)
    }
}

pub fn auth_user_from_headers(headers: &HeaderMap) -> Result<AuthUser, AuthHeaderError> {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return Err(AuthHeaderError::MissingAuthorization);
    };
    let Ok(raw) = value.to_str() else {
        return Err(AuthHeaderError::InvalidAuthorization);
    };
    let Some(token) = raw.strip_prefix("Bearer ").map(str::trim) else {
        return Err(AuthHeaderError::InvalidAuthorization);
    };
    let claims = verify_access_token(token).map_err(|_| AuthHeaderError::InvalidOrExpiredToken)?;
    Ok(AuthUser {
        user_id: claims.sub,
        email: claims.email,
    })
}

pub fn normalize_email(raw: &str) -> Option<String> {
    let email = raw.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return None;
    }
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() || !parts[1].contains('.') {
        return None;
    }
    Some(email)
}

pub fn validate_password(password: &str) -> Result<(), String> {
    if password.chars().count() < 8 {
        return Err("密码长度至少 8 位".to_string());
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, String> {
    validate_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| format!("密码加密失败: {err}"))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    let parsed = PasswordHash::new(hash).map_err(|err| format!("密码哈希格式无效: {err}"))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn sign_access_token(user_id: &str, email: &str) -> Result<String, String> {
    let cfg = Config::get();
    let now = chrono::Utc::now().timestamp() as usize;
    let ttl = cfg.auth_access_token_ttl_seconds.max(60) as usize;
    let claims = AccessTokenClaims {
        sub: user_id.to_string(),
        email: email.to_string(),
        iat: now,
        exp: now + ttl,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(cfg.auth_jwt_secret.as_bytes()),
    )
    .map_err(|err| format!("签发 token 失败: {err}"))
}

pub fn verify_access_token(token: &str) -> Result<AccessTokenClaims, String> {
    let cfg = Config::get();
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let data = decode::<AccessTokenClaims>(
        token,
        &DecodingKey::from_secret(cfg.auth_jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|err| format!("token 校验失败: {err}"))?;
    Ok(data.claims)
}

fn unauthorized(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": message
        })),
    )
}
