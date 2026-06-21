use std::sync::Arc;

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use crate::{config::AppConfig, state::AppState};

use super::operator_auth;

const BEARER_PREFIX: &str = "Bearer ";
const USER_ROLE_SUPER_ADMIN: &str = "super_admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserServiceClaims {
    iss: String,
    aud: String,
    sub: String,
    exp: usize,
    iat: usize,
    jti: String,
    principal_type: String,
    user_id: Option<String>,
    username: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    agent_account_id: Option<String>,
    owner_user_id: Option<String>,
    owner_username: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UserModelProfilePrincipal {
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub role: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
}

impl From<UserServiceClaims> for UserModelProfilePrincipal {
    fn from(value: UserServiceClaims) -> Self {
        Self {
            user_id: value.user_id,
            username: value.username,
            role: value.role,
            owner_user_id: value.owner_user_id,
            owner_username: value.owner_username,
        }
    }
}

impl UserModelProfilePrincipal {
    pub fn effective_owner_user_id(&self) -> Option<&str> {
        normalize_optional(self.user_id.as_deref()).or_else(|| normalize_optional(self.owner_user_id.as_deref()))
    }

    pub fn effective_owner_username(&self) -> Option<&str> {
        normalize_optional(self.username.as_deref())
            .or_else(|| normalize_optional(self.owner_username.as_deref()))
    }

    pub fn is_super_admin(&self) -> bool {
        self.role.as_deref() == Some(USER_ROLE_SUPER_ADMIN)
    }
}

#[derive(Debug, Clone)]
pub enum ModelProfileAuthContext {
    User(UserModelProfilePrincipal),
    Operator,
}

impl ModelProfileAuthContext {
    pub fn resolve_owner_scope(
        &self,
        requested_owner_user_id: Option<&str>,
    ) -> Result<Option<String>, (StatusCode, String)> {
        let requested_owner_user_id =
            normalize_optional(requested_owner_user_id).map(ToOwned::to_owned);
        match self {
            Self::User(principal) => {
                let effective_owner_user_id = principal.effective_owner_user_id().ok_or_else(|| {
                    (
                        StatusCode::UNAUTHORIZED,
                        "authenticated principal does not carry a user owner scope".to_string(),
                    )
                })?;
                if principal.is_super_admin() {
                    return Ok(requested_owner_user_id
                        .or_else(|| Some(effective_owner_user_id.to_string())));
                }
                if let Some(requested_owner_user_id) = requested_owner_user_id.as_deref() {
                    if requested_owner_user_id != effective_owner_user_id {
                        return Err((
                            StatusCode::FORBIDDEN,
                            "owner_user_id does not match authenticated user".to_string(),
                        ));
                    }
                }
                Ok(Some(effective_owner_user_id.to_string()))
            }
            Self::Operator => Ok(requested_owner_user_id),
        }
    }

    pub fn owner_username_for_create(&self) -> Option<String> {
        match self {
            Self::User(principal) => principal
                .effective_owner_username()
                .map(ToOwned::to_owned),
            Self::Operator => None,
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for ModelProfileAuthContext
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<ModelProfileAuthContext>()
            .cloned()
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    "missing model profile auth context".to_string(),
                )
            })
    }
}

pub async fn require_model_profile_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    if let Some(token) = bearer_token_from_request(&request)? {
        let principal = decode_user_service_principal(token.as_str(), &state.config)
            .map_err(|err| (StatusCode::UNAUTHORIZED, format!("invalid user token: {err}")))?;
        request
            .extensions_mut()
            .insert(ModelProfileAuthContext::User(principal));
        return Ok(next.run(request).await);
    }

    let Some(expected_token) = state.config.operator_token.as_deref() else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "missing authorization header".to_string(),
        ));
    };
    let provided = operator_auth::extract_operator_token(request.headers()).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "missing operator token".to_string(),
        )
    })?;
    if !operator_auth::constant_time_equal(expected_token, provided) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "invalid operator token".to_string(),
        ));
    }
    request
        .extensions_mut()
        .insert(ModelProfileAuthContext::Operator);
    Ok(next.run(request).await)
}

fn bearer_token_from_request(request: &Request<Body>) -> Result<Option<String>, (StatusCode, String)> {
    let Some(raw_value) = request.headers().get(AUTHORIZATION) else {
        return Ok(None);
    };
    let value = raw_value.to_str().map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            "invalid authorization header".to_string(),
        )
    })?;
    let token = value
        .strip_prefix(BEARER_PREFIX)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "invalid authorization header".to_string(),
            )
        })?;
    Ok(Some(token.to_string()))
}

fn decode_user_service_principal(
    token: &str,
    config: &AppConfig,
) -> Result<UserModelProfilePrincipal, String> {
    let jwt_secret = config
        .user_service_jwt_secret
        .as_deref()
        .ok_or_else(|| "MEMORY_ENGINE_USER_SERVICE_JWT_SECRET is not configured".to_string())?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[config.user_service_user_audience.as_str()]);
    validation.set_issuer(&[config.user_service_jwt_issuer.as_str()]);
    let data = decode::<UserServiceClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|err| err.to_string())?;
    Ok(UserModelProfilePrincipal::from(data.claims))
}

fn normalize_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{ModelProfileAuthContext, UserModelProfilePrincipal};
    use axum::http::StatusCode;

    fn principal(user_id: Option<&str>, owner_user_id: Option<&str>, role: Option<&str>) -> UserModelProfilePrincipal {
        UserModelProfilePrincipal {
            user_id: user_id.map(ToOwned::to_owned),
            username: Some("alice".to_string()),
            role: role.map(ToOwned::to_owned),
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: Some("alice".to_string()),
        }
    }

    #[test]
    fn normal_user_scope_is_locked_to_self() {
        let auth = ModelProfileAuthContext::User(principal(Some("user_a"), None, None));
        let scope = auth.resolve_owner_scope(None).expect("scope");
        assert_eq!(scope.as_deref(), Some("user_a"));

        let err = auth
            .resolve_owner_scope(Some("user_b"))
            .expect_err("should reject mismatched scope");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn super_admin_can_override_owner_scope() {
        let auth = ModelProfileAuthContext::User(principal(Some("admin"), None, Some("super_admin")));
        let scope = auth
            .resolve_owner_scope(Some("user_b"))
            .expect("scope");
        assert_eq!(scope.as_deref(), Some("user_b"));
    }

    #[test]
    fn operator_scope_can_fall_back_to_global() {
        let auth = ModelProfileAuthContext::Operator;
        assert_eq!(auth.resolve_owner_scope(None).expect("scope"), None);
    }
}
