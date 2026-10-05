// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::FromRequestParts;
use axum::http::{request::Parts, HeaderMap, StatusCode};
use axum::Json;
use chatos_service_runtime::{
    bearer_token_from_headers as parse_bearer_token_from_headers, BearerTokenError,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

use crate::config::AppConfig;
use crate::models::{AuthUser, UserRecord, PRINCIPAL_TYPE_HUMAN_USER, USER_ROLE_SUPER_ADMIN};

pub const MIN_PASSWORD_CHARACTERS: usize = 12;
pub const MAX_PASSWORD_CHARACTERS: usize = 128;

const COMMON_PASSWORDS: &[&str] = &[
    "123456789012",
    "adminadminadmin",
    "administrator",
    "letmeinplease",
    "password1234",
    "password12345",
    "password123456",
    "qwertyuiop12",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub jti: String,
    pub principal_type: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    #[serde(default)]
    pub credential_version: Option<i64>,
    pub agent_account_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IssuedUserToken {
    pub token: String,
    pub jti: String,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone)]
pub struct CurrentPrincipal {
    pub sub: String,
    pub jti: String,
    pub exp: usize,
    pub principal_type: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub credential_version: Option<i64>,
    pub agent_account_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub scopes: Vec<String>,
}

impl CurrentPrincipal {
    pub fn is_super_admin(&self) -> bool {
        self.role.as_deref() == Some(USER_ROLE_SUPER_ADMIN)
    }

    pub fn auth_user(&self) -> AuthUser {
        AuthUser {
            id: self
                .user_id
                .clone()
                .or_else(|| self.agent_account_id.clone())
                .unwrap_or_default(),
            username: self.username.clone().unwrap_or_default(),
            display_name: self
                .display_name
                .clone()
                .unwrap_or_else(|| self.username.clone().unwrap_or_default()),
            role: self.role.clone().unwrap_or_default(),
            principal_type: self.principal_type.clone(),
        }
    }
}

impl From<AuthClaims> for CurrentPrincipal {
    fn from(value: AuthClaims) -> Self {
        Self {
            sub: value.sub,
            jti: value.jti,
            exp: value.exp,
            principal_type: value.principal_type,
            user_id: value.user_id,
            username: value.username,
            display_name: value.display_name,
            role: value.role,
            credential_version: value.credential_version,
            agent_account_id: value.agent_account_id,
            owner_user_id: value.owner_user_id,
            owner_username: value.owner_username,
            owner_display_name: value.owner_display_name,
            scopes: value.scopes,
        }
    }
}

impl<S> FromRequestParts<S> for CurrentPrincipal
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CurrentPrincipal>()
            .cloned()
            .ok_or_else(|| unauthorized("missing authenticated principal"))
    }
}

pub fn normalize_username(value: &str) -> Result<String, String> {
    let username = value.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err("username is required".to_string());
    }
    if username.len() > 64 {
        return Err("username is too long".to_string());
    }
    Ok(username)
}

pub fn normalize_display_name(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback.to_string())
}

pub fn hash_password(password: &str) -> Result<String, String> {
    validate_new_password(password)?;
    let mut salt_bytes = [0_u8; 16];
    rand::fill(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|err| err.to_string())?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| err.to_string())
}

pub fn validate_new_password(password: &str) -> Result<(), String> {
    if password.trim().is_empty() {
        return Err("password is required".to_string());
    }
    let character_count = password.chars().take(MAX_PASSWORD_CHARACTERS + 1).count();
    if character_count < MIN_PASSWORD_CHARACTERS {
        return Err(format!(
            "password must contain at least {MIN_PASSWORD_CHARACTERS} characters"
        ));
    }
    if character_count > MAX_PASSWORD_CHARACTERS {
        return Err(format!(
            "password must contain at most {MAX_PASSWORD_CHARACTERS} characters"
        ));
    }
    let normalized = password.trim().to_ascii_lowercase();
    if COMMON_PASSWORDS.contains(&normalized.as_str()) {
        return Err("password is too common".to_string());
    }
    Ok(())
}

pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn credential_version_matches(token_version: Option<i64>, current_version: i64) -> bool {
    token_version.unwrap_or_default() == current_version
}

pub fn encode_user_token(config: &AppConfig, user: &UserRecord) -> Result<String, String> {
    issue_user_token(config, user, config.user_access_ttl_seconds).map(|issued| issued.token)
}

pub fn issue_user_token(
    config: &AppConfig,
    user: &UserRecord,
    ttl_seconds: i64,
) -> Result<IssuedUserToken, String> {
    issue_user_token_with_scopes(config, user, ttl_seconds, vec!["user_service".to_string()])
}

pub fn issue_user_token_with_scopes(
    config: &AppConfig,
    user: &UserRecord,
    ttl_seconds: i64,
    scopes: Vec<String>,
) -> Result<IssuedUserToken, String> {
    let issued_at = now_timestamp();
    let expires_at = (issued_at as i64 + ttl_seconds.max(60)).max(0);
    let jti = Uuid::new_v4().to_string();
    let token = encode_token(
        config,
        AuthClaims {
            iss: config.jwt_issuer.clone(),
            aud: config.user_service_audience.clone(),
            sub: format!("user:{}", user.id),
            exp: expires_at as usize,
            iat: issued_at,
            jti: jti.clone(),
            principal_type: PRINCIPAL_TYPE_HUMAN_USER.to_string(),
            user_id: Some(user.id.clone()),
            username: Some(user.username.clone()),
            display_name: Some(user.display_name.clone()),
            role: Some(user.role.clone()),
            credential_version: Some(user.credential_version),
            agent_account_id: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            scopes,
        },
    )?;
    Ok(IssuedUserToken {
        token,
        jti,
        expires_at_unix: expires_at,
    })
}

pub fn decode_user_service_token(token: &str, config: &AppConfig) -> Result<AuthClaims, String> {
    decode_token(token, config, config.user_service_audience.as_str())
}

pub fn bearer_token_from_headers(headers: &HeaderMap) -> Result<String, String> {
    match parse_bearer_token_from_headers(headers) {
        Ok(token) => Ok(token.to_string()),
        Err(BearerTokenError::MissingAuthorizationHeader) => {
            Err("missing authorization header".to_string())
        }
        Err(
            BearerTokenError::InvalidAuthorizationHeader | BearerTokenError::InvalidBearerToken,
        ) => Err("invalid authorization header".to_string()),
    }
}

fn encode_token(config: &AppConfig, claims: AuthClaims) -> Result<String, String> {
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|err| err.to_string())
}

fn decode_token(token: &str, config: &AppConfig, audience: &str) -> Result<AuthClaims, String> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(&[config.jwt_issuer.as_str()]);
    let data = decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|err| err.to_string())?;
    Ok(data.claims)
}

fn now_timestamp() -> usize {
    Utc::now().timestamp().max(0) as usize
}

pub fn unauthorized(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": message })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_password_policy_rejects_short_long_blank_and_common_values() {
        assert_eq!(
            validate_new_password("short").unwrap_err(),
            "password must contain at least 12 characters"
        );
        assert_eq!(
            validate_new_password(&"x".repeat(MAX_PASSWORD_CHARACTERS + 1)).unwrap_err(),
            "password must contain at most 128 characters"
        );
        assert_eq!(
            validate_new_password("            ").unwrap_err(),
            "password is required"
        );
        assert_eq!(
            validate_new_password("Password1234").unwrap_err(),
            "password is too common"
        );
    }

    #[test]
    fn compliant_password_is_hashed_and_verified() {
        let password = "correct horse battery staple";
        let hash = hash_password(password).expect("hash compliant password");
        assert!(verify_password(password, &hash));
        assert!(!verify_password("different password value", &hash));
    }

    #[test]
    fn credential_version_invalidates_tokens_after_password_change() {
        assert!(credential_version_matches(Some(3), 3));
        assert!(!credential_version_matches(Some(2), 3));
        assert!(credential_version_matches(None, 0));
        assert!(!credential_version_matches(None, 1));
    }
}
