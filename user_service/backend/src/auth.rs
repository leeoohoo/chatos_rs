use axum::extract::FromRequestParts;
use axum::http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode};
use axum::Json;
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

use crate::config::AppConfig;
use crate::models::{
    AgentAccountRecord, AuthUser, UserRecord, PRINCIPAL_TYPE_AGENT_ACCOUNT,
    PRINCIPAL_TYPE_HUMAN_USER, USER_ROLE_SUPER_ADMIN,
};

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
    pub agent_account_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
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
            agent_account_id: value.agent_account_id,
            owner_user_id: value.owner_user_id,
            owner_username: value.owner_username,
            owner_display_name: value.owner_display_name,
            scopes: value.scopes,
        }
    }
}

#[axum::async_trait]
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
    if password.trim().is_empty() {
        return Err("password is required".to_string());
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| err.to_string())
}

pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn encode_user_token(config: &AppConfig, user: &UserRecord) -> Result<String, String> {
    encode_token(
        config,
        AuthClaims {
            iss: config.jwt_issuer.clone(),
            aud: config.user_service_audience.clone(),
            sub: format!("user:{}", user.id),
            exp: expiry_timestamp(config.user_access_ttl_seconds),
            iat: now_timestamp(),
            jti: Uuid::new_v4().to_string(),
            principal_type: PRINCIPAL_TYPE_HUMAN_USER.to_string(),
            user_id: Some(user.id.clone()),
            username: Some(user.username.clone()),
            display_name: Some(user.display_name.clone()),
            role: Some(user.role.clone()),
            agent_account_id: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            scopes: vec!["user_service".to_string()],
        },
    )
}

pub fn encode_agent_token(
    config: &AppConfig,
    agent: &AgentAccountRecord,
    owner: &UserRecord,
) -> Result<String, String> {
    encode_token(
        config,
        AuthClaims {
            iss: config.jwt_issuer.clone(),
            aud: config.task_runner_audience.clone(),
            sub: format!("agent:{}", agent.id),
            exp: expiry_timestamp(config.task_runner_access_ttl_seconds),
            iat: now_timestamp(),
            jti: Uuid::new_v4().to_string(),
            principal_type: PRINCIPAL_TYPE_AGENT_ACCOUNT.to_string(),
            user_id: None,
            username: Some(agent.username.clone()),
            display_name: Some(agent.display_name.clone()),
            role: None,
            agent_account_id: Some(agent.id.clone()),
            owner_user_id: Some(owner.id.clone()),
            owner_username: Some(owner.username.clone()),
            owner_display_name: Some(owner.display_name.clone()),
            scopes: vec!["task_runner".to_string()],
        },
    )
}

pub fn decode_any_user_service_token(
    token: &str,
    config: &AppConfig,
) -> Result<AuthClaims, String> {
    match decode_token(token, config, config.user_service_audience.as_str()) {
        Ok(claims) => Ok(claims),
        Err(user_err) => decode_token(token, config, config.task_runner_audience.as_str())
            .map_err(|task_err| format!("{user_err}; {task_err}")),
    }
}

pub fn bearer_token_from_headers(headers: &HeaderMap) -> Result<String, String> {
    let value = headers
        .get(AUTHORIZATION)
        .ok_or_else(|| "missing authorization header".to_string())?
        .to_str()
        .map_err(|_| "invalid authorization header".to_string())?;
    let token = value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "invalid authorization header".to_string())?;
    Ok(token.to_string())
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

fn expiry_timestamp(ttl_seconds: i64) -> usize {
    (Utc::now().timestamp() + ttl_seconds.max(60)).max(0) as usize
}

pub fn unauthorized(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": message })))
}
