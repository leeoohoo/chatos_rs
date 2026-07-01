// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};

use crate::{config::AppConfig, state::AppState};

use super::operator_auth;

const BEARER_PREFIX: &str = "Bearer ";
const PRINCIPAL_TYPE_AGENT_ACCOUNT: &str = "agent_account";
const PRINCIPAL_TYPE_HUMAN_USER: &str = "human_user";
const USER_ROLE_SUPER_ADMIN: &str = "super_admin";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserServiceVerifiedPrincipal {
    principal_type: String,
    user_id: Option<String>,
    username: Option<String>,
    role: Option<String>,
    owner_user_id: Option<String>,
    owner_username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserServiceVerifyResponse {
    principal: UserServiceVerifiedPrincipal,
}

#[derive(Debug, Clone)]
pub struct MemoryPrincipal {
    pub principal_type: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub role: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
}

impl From<UserServiceVerifiedPrincipal> for MemoryPrincipal {
    fn from(value: UserServiceVerifiedPrincipal) -> Self {
        Self {
            principal_type: value.principal_type,
            user_id: value.user_id,
            username: value.username,
            role: value.role,
            owner_user_id: value.owner_user_id,
            owner_username: value.owner_username,
        }
    }
}

impl MemoryPrincipal {
    pub fn effective_owner_user_id(&self) -> Option<&str> {
        if self.principal_type == PRINCIPAL_TYPE_AGENT_ACCOUNT {
            return normalize_optional(self.owner_user_id.as_deref())
                .or_else(|| normalize_optional(self.user_id.as_deref()));
        }
        normalize_optional(self.user_id.as_deref())
            .or_else(|| normalize_optional(self.owner_user_id.as_deref()))
    }

    pub fn effective_owner_username(&self) -> Option<&str> {
        if self.principal_type == PRINCIPAL_TYPE_AGENT_ACCOUNT {
            return normalize_optional(self.owner_username.as_deref())
                .or_else(|| normalize_optional(self.username.as_deref()));
        }
        normalize_optional(self.username.as_deref())
            .or_else(|| normalize_optional(self.owner_username.as_deref()))
    }

    pub fn is_super_admin(&self) -> bool {
        self.principal_type == PRINCIPAL_TYPE_HUMAN_USER
            && self.role.as_deref() == Some(USER_ROLE_SUPER_ADMIN)
    }
}

#[derive(Debug, Clone)]
pub enum MemoryAuthContext {
    User(MemoryPrincipal),
    Operator,
}

impl MemoryAuthContext {
    pub fn resolve_owner_scope(
        &self,
        requested_owner_user_id: Option<&str>,
    ) -> Result<Option<String>, (StatusCode, String)> {
        let requested_owner_user_id =
            normalize_optional(requested_owner_user_id).map(ToOwned::to_owned);
        match self {
            Self::User(principal) => {
                let effective_owner_user_id =
                    principal.effective_owner_user_id().ok_or_else(|| {
                        (
                            StatusCode::UNAUTHORIZED,
                            "authenticated principal does not carry a user owner scope".to_string(),
                        )
                    })?;
                if principal.is_super_admin() {
                    return Ok(requested_owner_user_id);
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
            Self::User(principal) => principal.effective_owner_username().map(ToOwned::to_owned),
            Self::Operator => None,
        }
    }

    pub fn is_super_admin_or_operator(&self) -> bool {
        match self {
            Self::Operator => true,
            Self::User(principal) => principal.is_super_admin(),
        }
    }

    pub fn require_super_admin_or_operator(&self) -> Result<(), (StatusCode, String)> {
        if self.is_super_admin_or_operator() {
            Ok(())
        } else {
            Err((
                StatusCode::FORBIDDEN,
                "super_admin permission required".to_string(),
            ))
        }
    }

    pub fn ensure_tenant_scope(&self, tenant_id: &str) -> Result<(), (StatusCode, String)> {
        let tenant_id = normalize_optional(Some(tenant_id))
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "tenant_id is required".to_string()))?;
        match self {
            Self::Operator => Ok(()),
            Self::User(principal) if principal.is_super_admin() => Ok(()),
            Self::User(principal) => {
                let effective_owner_user_id =
                    principal.effective_owner_user_id().ok_or_else(|| {
                        (
                            StatusCode::UNAUTHORIZED,
                            "authenticated principal does not carry a tenant scope".to_string(),
                        )
                    })?;
                if tenant_id == effective_owner_user_id {
                    Ok(())
                } else {
                    Err((
                        StatusCode::FORBIDDEN,
                        "tenant_id does not match authenticated user".to_string(),
                    ))
                }
            }
        }
    }

    pub fn resolve_tenant_scope(
        &self,
        requested_tenant_id: Option<&str>,
    ) -> Result<Option<String>, (StatusCode, String)> {
        let requested_tenant_id = normalize_optional(requested_tenant_id).map(ToOwned::to_owned);
        match self {
            Self::Operator => Ok(requested_tenant_id),
            Self::User(principal) if principal.is_super_admin() => Ok(requested_tenant_id),
            Self::User(principal) => {
                let effective_owner_user_id =
                    principal.effective_owner_user_id().ok_or_else(|| {
                        (
                            StatusCode::UNAUTHORIZED,
                            "authenticated principal does not carry a tenant scope".to_string(),
                        )
                    })?;
                if let Some(requested_tenant_id) = requested_tenant_id.as_deref() {
                    if requested_tenant_id != effective_owner_user_id {
                        return Err((
                            StatusCode::FORBIDDEN,
                            "tenant_id does not match authenticated user".to_string(),
                        ));
                    }
                }
                Ok(Some(effective_owner_user_id.to_string()))
            }
        }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for MemoryAuthContext
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<MemoryAuthContext>()
            .cloned()
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    "missing memory auth context".to_string(),
                )
            })
    }
}

pub async fn require_memory_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    if let Some(token) = bearer_token_from_request(&request)? {
        match verify_user_service_principal(token.as_str(), &state.config).await {
            Ok(principal) => {
                request
                    .extensions_mut()
                    .insert(MemoryAuthContext::User(principal));
                return Ok(next.run(request).await);
            }
            Err(err) => {
                if state
                    .config
                    .operator_token
                    .as_deref()
                    .is_some_and(|expected| operator_auth::constant_time_equal(expected, &token))
                {
                    request.extensions_mut().insert(MemoryAuthContext::Operator);
                    return Ok(next.run(request).await);
                }
                return Err(err);
            }
        }
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
    request.extensions_mut().insert(MemoryAuthContext::Operator);
    Ok(next.run(request).await)
}

fn bearer_token_from_request(
    request: &Request<Body>,
) -> Result<Option<String>, (StatusCode, String)> {
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

async fn verify_user_service_principal(
    token: &str,
    config: &AppConfig,
) -> Result<MemoryPrincipal, (StatusCode, String)> {
    let endpoint = format!(
        "{}/api/auth/verify",
        config.user_service_base_url.trim().trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(
            config.user_service_request_timeout_ms.max(300),
        ))
        .build()
        .map_err(|err| {
            (
                StatusCode::BAD_GATEWAY,
                format!("build user_service client failed: {err}"),
            )
        })?;
    let response = client
        .get(endpoint)
        .bearer_auth(token.trim())
        .send()
        .await
        .map_err(|err| {
            (
                StatusCode::BAD_GATEWAY,
                format!("verify token via user_service failed: {err}"),
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::UNAUTHORIZED,
            format!("invalid user token: {} {}", status.as_u16(), body),
        ));
    }
    let payload = response
        .json::<UserServiceVerifyResponse>()
        .await
        .map_err(|err| {
            (
                StatusCode::BAD_GATEWAY,
                format!("parse user_service verify response failed: {err}"),
            )
        })?;
    match payload.principal.principal_type.as_str() {
        PRINCIPAL_TYPE_HUMAN_USER | PRINCIPAL_TYPE_AGENT_ACCOUNT => {}
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                "memory_engine requires a human user or agent account token".to_string(),
            ));
        }
    }
    Ok(MemoryPrincipal::from(payload.principal))
}

fn normalize_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{MemoryAuthContext, MemoryPrincipal};
    use axum::http::StatusCode;

    fn principal(
        principal_type: &str,
        user_id: Option<&str>,
        owner_user_id: Option<&str>,
        role: Option<&str>,
    ) -> MemoryPrincipal {
        MemoryPrincipal {
            principal_type: principal_type.to_string(),
            user_id: user_id.map(ToOwned::to_owned),
            username: Some("alice".to_string()),
            role: role.map(ToOwned::to_owned),
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: Some("alice".to_string()),
        }
    }

    #[test]
    fn normal_user_scope_is_locked_to_self() {
        let auth = MemoryAuthContext::User(principal("human_user", Some("user_a"), None, None));
        let scope = auth.resolve_owner_scope(None).expect("scope");
        assert_eq!(scope.as_deref(), Some("user_a"));

        let err = auth
            .resolve_owner_scope(Some("user_b"))
            .expect_err("should reject mismatched scope");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn agent_scope_uses_owner_user_id() {
        let auth = MemoryAuthContext::User(principal("agent_account", None, Some("user_a"), None));
        assert!(auth.ensure_tenant_scope("user_a").is_ok());
        let err = auth
            .ensure_tenant_scope("user_b")
            .expect_err("should reject mismatched tenant");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn super_admin_can_override_owner_scope() {
        let auth = MemoryAuthContext::User(principal(
            "human_user",
            Some("admin"),
            None,
            Some("super_admin"),
        ));
        let scope = auth.resolve_owner_scope(Some("user_b")).expect("scope");
        assert_eq!(scope.as_deref(), Some("user_b"));
    }

    #[test]
    fn operator_scope_can_fall_back_to_global() {
        let auth = MemoryAuthContext::Operator;
        assert_eq!(auth.resolve_owner_scope(None).expect("scope"), None);
        assert_eq!(auth.resolve_tenant_scope(None).expect("scope"), None);
    }
}
