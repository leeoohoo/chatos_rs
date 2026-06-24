use axum::http::header::AUTHORIZATION;
use axum::http::HeaderMap;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::models::{AgentAccountListItem, AuthUser, LoginRequest, LoginResponse, UserRole};

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub principal_type: String,
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccessToken(pub String);

impl CurrentUser {
    pub fn public_user(&self) -> AuthUser {
        AuthUser {
            id: self.id.clone(),
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            role: self.role,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.role == UserRole::Admin
    }

    pub fn is_human_user(&self) -> bool {
        self.principal_type == "human_user"
    }

    pub fn is_agent_account(&self) -> bool {
        self.principal_type == "agent_account"
    }

    pub fn with_owner_identity_from(mut self, owner: &CurrentUser) -> Self {
        self.owner_user_id = owner.effective_owner_user_id().map(ToOwned::to_owned);
        self.owner_username = owner.effective_owner_username().map(ToOwned::to_owned);
        self.owner_display_name = owner
            .effective_owner_display_name()
            .map(ToOwned::to_owned)
            .or_else(|| owner.effective_owner_username().map(ToOwned::to_owned));
        self
    }

    pub fn effective_owner_user_id(&self) -> Option<&str> {
        self.owner_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn effective_owner_username(&self) -> Option<&str> {
        self.owner_username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn effective_owner_display_name(&self) -> Option<&str> {
        self.owner_display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn can_access_owned_resource(&self, owner_user_id: Option<&str>) -> bool {
        if self.is_admin() {
            return true;
        }
        let owner_user_id = owner_user_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        owner_user_id.is_some() && self.effective_owner_user_id() == owner_user_id
    }
}

#[derive(Debug, Serialize)]
struct UserServiceLoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Debug, Deserialize)]
struct UserServiceAuthUser {
    id: String,
    username: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    principal_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserServiceLoginResponse {
    token: String,
    user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize)]
struct UserServiceVerifiedPrincipal {
    principal_type: String,
    user_id: Option<String>,
    username: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    agent_account_id: Option<String>,
    owner_user_id: Option<String>,
    owner_username: Option<String>,
    owner_display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserServiceVerifyResponse {
    principal: UserServiceVerifiedPrincipal,
}

pub async fn login_via_user_service(
    config: &AppConfig,
    input: &LoginRequest,
) -> Result<LoginResponse, String> {
    let payload: UserServiceLoginResponse = request_user_service_json(
        config,
        Method::POST,
        "/api/auth/login",
        None,
        Some(&UserServiceLoginRequest {
            username: input.username.as_str(),
            password: input.password.as_str(),
        }),
    )
    .await?;
    let user = current_user_from_user_service_auth_user(payload.user)?.public_user();
    Ok(LoginResponse {
        token: payload.token,
        user,
    })
}

pub async fn verify_token_via_user_service(
    config: &AppConfig,
    token: &str,
) -> Result<CurrentUser, String> {
    let payload: UserServiceVerifyResponse = request_user_service_json::<(), _>(
        config,
        Method::GET,
        "/api/auth/verify",
        Some(token),
        None,
    )
    .await?;
    current_user_from_verified_principal(payload.principal)
}

pub async fn list_agent_accounts_via_user_service(
    config: &AppConfig,
    token: &str,
) -> Result<Vec<AgentAccountListItem>, String> {
    request_user_service_json::<(), _>(
        config,
        Method::GET,
        "/api/agent-accounts",
        Some(token),
        None,
    )
    .await
}

pub fn bearer_token_from_headers(headers: &HeaderMap) -> Result<&str, String> {
    let value = headers
        .get(AUTHORIZATION)
        .ok_or_else(|| "缺少登录令牌".to_string())?
        .to_str()
        .map_err(|_| "登录令牌格式不正确".to_string())?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next().unwrap_or_default();
    let token = parts.next().unwrap_or_default();
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || parts.next().is_some() {
        return Err("登录令牌格式不正确".to_string());
    }
    Ok(token)
}

async fn request_user_service_json<TBody, TResp>(
    config: &AppConfig,
    method: Method,
    path: &str,
    access_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<TResp, String>
where
    TBody: Serialize + ?Sized,
    TResp: serde::de::DeserializeOwned,
{
    let endpoint = format!(
        "{}{}",
        config.user_service_base_url.trim().trim_end_matches('/'),
        path
    );
    let client = reqwest::Client::builder()
        .timeout(config.user_service_request_timeout)
        .build()
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let mut request = client.request(method, endpoint);
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token.trim());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("user_service request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(if text.trim().is_empty() {
            format!("user_service request failed with status {status}")
        } else {
            text
        });
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| format!("parse user_service response failed: {err}"))
}

fn current_user_from_user_service_auth_user(
    user: UserServiceAuthUser,
) -> Result<CurrentUser, String> {
    let principal_type = user
        .principal_type
        .as_deref()
        .unwrap_or("human_user")
        .trim();
    if principal_type != "human_user" {
        return Err("project service login requires a human user".to_string());
    }
    let username = normalize_identity_text(user.username.as_deref()).unwrap_or(user.id.as_str());
    Ok(CurrentUser {
        principal_type: "human_user".to_string(),
        id: user.id.clone(),
        username: username.to_string(),
        display_name: normalize_identity_text(user.display_name.as_deref())
            .unwrap_or(username)
            .to_string(),
        role: map_user_service_role(user.role.as_deref()),
        owner_user_id: Some(user.id.clone()),
        owner_username: Some(username.to_string()),
        owner_display_name: normalize_identity_text(user.display_name.as_deref())
            .map(ToOwned::to_owned),
    })
}

fn current_user_from_verified_principal(
    principal: UserServiceVerifiedPrincipal,
) -> Result<CurrentUser, String> {
    match principal.principal_type.as_str() {
        "human_user" => {
            let user_id = normalize_identity_text(principal.user_id.as_deref())
                .ok_or_else(|| "token missing user identity".to_string())?;
            let username =
                normalize_identity_text(principal.username.as_deref()).unwrap_or(user_id);
            Ok(CurrentUser {
                principal_type: "human_user".to_string(),
                id: user_id.to_string(),
                username: username.to_string(),
                display_name: normalize_identity_text(principal.display_name.as_deref())
                    .unwrap_or(username)
                    .to_string(),
                role: map_user_service_role(principal.role.as_deref()),
                owner_user_id: Some(user_id.to_string()),
                owner_username: Some(username.to_string()),
                owner_display_name: normalize_identity_text(principal.display_name.as_deref())
                    .map(ToOwned::to_owned),
            })
        }
        "agent_account" => {
            let agent_account_id = normalize_identity_text(principal.agent_account_id.as_deref())
                .ok_or_else(|| "token missing agent identity".to_string())?;
            let username = normalize_identity_text(principal.username.as_deref())
                .or_else(|| normalize_identity_text(principal.owner_username.as_deref()))
                .unwrap_or(agent_account_id);
            Ok(CurrentUser {
                principal_type: "agent_account".to_string(),
                id: agent_account_id.to_string(),
                username: username.to_string(),
                display_name: normalize_identity_text(principal.display_name.as_deref())
                    .unwrap_or(username)
                    .to_string(),
                role: UserRole::Agent,
                owner_user_id: normalize_identity_text(principal.owner_user_id.as_deref())
                    .map(ToOwned::to_owned),
                owner_username: normalize_identity_text(principal.owner_username.as_deref())
                    .map(ToOwned::to_owned),
                owner_display_name: normalize_identity_text(
                    principal.owner_display_name.as_deref(),
                )
                .map(ToOwned::to_owned),
            })
        }
        _ => Err("unsupported principal type".to_string()),
    }
}

fn map_user_service_role(role: Option<&str>) -> UserRole {
    if role.map(str::trim) == Some("super_admin") {
        UserRole::Admin
    } else {
        UserRole::Agent
    }
}

fn normalize_identity_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
