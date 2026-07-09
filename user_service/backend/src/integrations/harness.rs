// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use tracing::{info, warn};

use crate::models::{
    HarnessProvisioningRecord, UserRecord, HARNESS_PROVISIONING_STATUS_FAILED,
    HARNESS_PROVISIONING_STATUS_PENDING, HARNESS_PROVISIONING_STATUS_PROVISIONED,
};
use crate::secrets::encrypt_secret;
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::http::{build_client_with_timeout, extract_error_message, normalized_url};

mod identifiers;
mod repo;

pub use repo::{
    create_harness_project_repo, get_harness_api_access_for_user, HarnessApiAccessResponse,
    HarnessProjectRepoCreateRequest, HarnessProjectRepoResponse,
};

use identifiers::{
    harness_email_for_user, harness_project_pat_identifier, harness_space_identifier_for_user,
    harness_uid_for_user, truncate_error,
};

#[derive(Debug, Clone)]
struct HarnessProvisioningIdentity {
    uid: String,
    email: String,
    space_identifier: String,
}

#[derive(Debug, Serialize)]
struct HarnessRegisterRequest<'a> {
    uid: &'a str,
    email: &'a str,
    display_name: &'a str,
    password: &'a str,
}

#[derive(Debug, Serialize)]
struct HarnessLoginRequest<'a> {
    login_identifier: &'a str,
    password: &'a str,
}

#[derive(Debug, Serialize)]
struct HarnessCreateSpaceRequest<'a> {
    identifier: &'a str,
    parent_ref: &'a str,
    description: &'a str,
    is_public: bool,
}

#[derive(Debug, Serialize)]
struct HarnessCreateAccessTokenRequest<'a> {
    identifier: &'a str,
}

#[derive(Debug, Deserialize)]
struct HarnessTokenResponse {
    access_token: String,
    #[serde(default)]
    token: Option<HarnessTokenRecord>,
}

#[derive(Debug, Deserialize)]
struct HarnessTokenRecord {
    identifier: String,
}

#[derive(Debug, Deserialize)]
struct HarnessCurrentUserResponse {
    uid: String,
}

struct HarnessAuthenticatedUser {
    access_token: String,
    resolved_uid: String,
}

#[derive(Debug)]
struct HarnessRequestError {
    status: Option<StatusCode>,
    message: String,
}

impl HarnessRequestError {
    fn from_error(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }

    fn is_already_exists(&self) -> bool {
        let message = self.message.to_ascii_lowercase();
        self.status == Some(StatusCode::CONFLICT)
            || message.contains("already")
            || message.contains("exist")
            || message.contains("duplicate")
            || message.contains("unique")
    }
}

impl fmt::Display for HarnessRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(status) = self.status {
            write!(f, "{} {}", status.as_u16(), self.message)
        } else {
            f.write_str(self.message.as_str())
        }
    }
}

pub async fn provision_harness_user_public_register(
    state: &AppState,
    user: &UserRecord,
    password: &str,
) -> Vec<String> {
    match provision_harness_user_public_register_result(state, user, password).await {
        Ok(()) => Vec::new(),
        Err(err) => vec![format!("harness provisioning failed: {err}")],
    }
}

pub async fn ensure_harness_user_public_register_on_login(
    state: &AppState,
    user: &UserRecord,
    password: &str,
) -> Vec<String> {
    if !state.config.harness_provisioning_enabled {
        return Vec::new();
    }
    match state
        .store
        .find_harness_provisioning_by_user_id(user.id.as_str())
        .await
    {
        Ok(Some(record))
            if record.status == HARNESS_PROVISIONING_STATUS_PROVISIONED
                && record
                    .encrypted_access_token
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|value| !value.is_empty()) =>
        {
            Vec::new()
        }
        Ok(_) => provision_harness_user_public_register(state, user, password).await,
        Err(err) => vec![format!("harness provisioning lookup failed: {err}")],
    }
}

pub async fn provision_harness_user_public_register_result(
    state: &AppState,
    user: &UserRecord,
    password: &str,
) -> Result<(), String> {
    if !state.config.harness_provisioning_enabled {
        return Ok(());
    }

    let identity = HarnessProvisioningIdentity::from_user(user, state);
    let attempt = begin_harness_provisioning_attempt(state, user, &identity, password).await?;
    let Some(base_url) = normalized_url(state.config.harness_base_url.as_deref()) else {
        warn!("harness provisioning enabled but HARNESS_BASE_URL is not configured");
        let err = "HARNESS_BASE_URL is not configured".to_string();
        let _ = finish_harness_provisioning_failure(state, attempt, err.as_str()).await;
        return Err(err);
    };

    let result = provision_harness_user_public_register_inner(
        state,
        base_url.as_str(),
        &identity,
        user,
        password,
    )
    .await;

    match result {
        Ok(token) => {
            let resolved_harness_uid = token.resolved_harness_uid.clone();
            finish_harness_provisioning_success(state, attempt, token).await?;
            info!(
                user_id = user.id.as_str(),
                username = user.username.as_str(),
                harness_uid = resolved_harness_uid.as_str(),
                harness_space = identity.space_identifier.as_str(),
                "harness user provisioning completed"
            );
            Ok(())
        }
        Err(err) => {
            let _ = finish_harness_provisioning_failure(state, attempt, err.as_str()).await;
            warn!(
                user_id = user.id.as_str(),
                username = user.username.as_str(),
                harness_uid = identity.uid.as_str(),
                harness_space = identity.space_identifier.as_str(),
                error = err.as_str(),
                "harness user provisioning failed"
            );
            Err(err)
        }
    }
}

impl HarnessProvisioningIdentity {
    fn from_user(user: &UserRecord, state: &AppState) -> Self {
        let uid = harness_uid_for_user(user);
        let email = harness_email_for_user(
            user,
            uid.as_str(),
            state.config.harness_synthetic_email_domain.as_str(),
        );
        let space_identifier = harness_space_identifier_for_user(
            user,
            uid.as_str(),
            state.config.harness_space_prefix.as_str(),
        );
        Self {
            uid,
            email,
            space_identifier,
        }
    }
}

async fn begin_harness_provisioning_attempt(
    state: &AppState,
    user: &UserRecord,
    identity: &HarnessProvisioningIdentity,
    password: &str,
) -> Result<HarnessProvisioningRecord, String> {
    let now = now_rfc3339();
    let prior = state
        .store
        .find_harness_provisioning_by_user_id(user.id.as_str())
        .await?;
    let attempts = prior.as_ref().map(|item| item.attempts + 1).unwrap_or(1);
    let created_at = prior
        .as_ref()
        .map(|item| item.created_at.clone())
        .unwrap_or_else(|| now.clone());
    let encrypted_password = encrypt_secret(password)?;
    let record = HarnessProvisioningRecord {
        user_id: user.id.clone(),
        username: user.username.clone(),
        harness_uid: identity.uid.clone(),
        harness_email: identity.email.clone(),
        space_identifier: identity.space_identifier.clone(),
        status: HARNESS_PROVISIONING_STATUS_PENDING.to_string(),
        attempts,
        encrypted_password: Some(encrypted_password),
        encrypted_access_token: prior
            .as_ref()
            .and_then(|item| item.encrypted_access_token.clone()),
        access_token_identifier: prior
            .as_ref()
            .and_then(|item| item.access_token_identifier.clone()),
        access_token_created_at: prior
            .as_ref()
            .and_then(|item| item.access_token_created_at.clone()),
        last_error: None,
        last_attempt_at: Some(now.clone()),
        provisioned_at: None,
        created_at,
        updated_at: now,
    };
    state.store.save_harness_provisioning(&record).await
}

async fn finish_harness_provisioning_success(
    state: &AppState,
    mut record: HarnessProvisioningRecord,
    token: HarnessCreatedAccessToken,
) -> Result<(), String> {
    let now = now_rfc3339();
    record.status = HARNESS_PROVISIONING_STATUS_PROVISIONED.to_string();
    if !token.resolved_harness_uid.trim().is_empty() {
        record.harness_uid = token.resolved_harness_uid;
    }
    record.encrypted_password = None;
    record.encrypted_access_token = Some(encrypt_secret(token.access_token.as_str())?);
    record.access_token_identifier = Some(token.identifier);
    record.access_token_created_at = Some(now.clone());
    record.last_error = None;
    record.provisioned_at = Some(now.clone());
    record.updated_at = now;
    state.store.save_harness_provisioning(&record).await?;
    Ok(())
}

async fn finish_harness_provisioning_failure(
    state: &AppState,
    mut record: HarnessProvisioningRecord,
    error: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    record.status = HARNESS_PROVISIONING_STATUS_FAILED.to_string();
    record.last_error = Some(truncate_error(error));
    record.updated_at = now;
    state.store.save_harness_provisioning(&record).await?;
    Ok(())
}

async fn provision_harness_user_public_register_inner(
    state: &AppState,
    base_url: &str,
    identity: &HarnessProvisioningIdentity,
    user: &UserRecord,
    password: &str,
) -> Result<HarnessCreatedAccessToken, String> {
    let authenticated = register_or_login_harness_user(state, base_url, identity, user, password)
        .await
        .map_err(|err| format!("create or login harness user failed: {err}"))?;
    ensure_harness_root_space(
        state,
        base_url,
        authenticated.access_token.as_str(),
        identity,
        user.username.as_str(),
    )
    .await
    .map_err(|err| format!("create harness root space failed: {err}"))?;
    let mut token = create_harness_project_access_token(
        state,
        base_url,
        authenticated.access_token.as_str(),
        identity,
        user,
    )
    .await
    .map_err(|err| format!("create harness project access token failed: {err}"))?;
    token.resolved_harness_uid = authenticated.resolved_uid;
    Ok(token)
}

async fn register_or_login_harness_user(
    state: &AppState,
    base_url: &str,
    identity: &HarnessProvisioningIdentity,
    user: &UserRecord,
    password: &str,
) -> Result<HarnessAuthenticatedUser, HarnessRequestError> {
    let register_body = HarnessRegisterRequest {
        uid: identity.uid.as_str(),
        email: identity.email.as_str(),
        display_name: user.display_name.as_str(),
        password,
    };
    let register_endpoint = format!("{base_url}/api/v1/register");
    match harness_request_json::<HarnessTokenResponse, _>(
        state,
        Method::POST,
        register_endpoint.as_str(),
        None,
        Some(&register_body),
    )
    .await
    {
        Ok(response) => Ok(HarnessAuthenticatedUser {
            access_token: non_empty_access_token(response)?,
            resolved_uid: identity.uid.clone(),
        }),
        Err(err) if err.is_already_exists() => {
            login_existing_harness_user(state, base_url, identity, password).await
        }
        Err(err) => Err(err),
    }
}

async fn login_existing_harness_user(
    state: &AppState,
    base_url: &str,
    identity: &HarnessProvisioningIdentity,
    password: &str,
) -> Result<HarnessAuthenticatedUser, HarnessRequestError> {
    match login_harness_user(state, base_url, identity.uid.as_str(), password).await {
        Ok(access_token) => Ok(HarnessAuthenticatedUser {
            access_token,
            resolved_uid: identity.uid.clone(),
        }),
        Err(uid_err) => match login_harness_user(state, base_url, identity.email.as_str(), password)
            .await
        {
            Ok(access_token) => {
                let resolved_uid =
                    fetch_harness_current_user_uid(state, base_url, access_token.as_str())
                        .await
                        .unwrap_or_else(|_| identity.email.clone());
                Ok(HarnessAuthenticatedUser {
                    access_token,
                    resolved_uid,
                })
            }
            Err(email_err) => Err(HarnessRequestError::from_error(format!(
                "harness user already exists, but login failed with uid '{}' ({}) and email '{}' ({}); use the existing Harness password or reset the Harness account password",
                identity.uid, uid_err, identity.email, email_err
            ))),
        },
    }
}

async fn login_harness_user(
    state: &AppState,
    base_url: &str,
    uid: &str,
    password: &str,
) -> Result<String, HarnessRequestError> {
    let login_body = HarnessLoginRequest {
        login_identifier: uid,
        password,
    };
    let login_endpoint = format!("{base_url}/api/v1/login");
    let response = harness_request_json::<HarnessTokenResponse, _>(
        state,
        Method::POST,
        login_endpoint.as_str(),
        None,
        Some(&login_body),
    )
    .await?;
    non_empty_access_token(response)
}

async fn fetch_harness_current_user_uid(
    state: &AppState,
    base_url: &str,
    access_token: &str,
) -> Result<String, HarnessRequestError> {
    let endpoint = format!("{base_url}/api/v1/user");
    let response = harness_request_json::<HarnessCurrentUserResponse, ()>(
        state,
        Method::GET,
        endpoint.as_str(),
        Some(access_token),
        None,
    )
    .await?;
    let uid = response.uid.trim().to_string();
    if uid.is_empty() {
        Err(HarnessRequestError::from_error(
            "harness current user response missing uid",
        ))
    } else {
        Ok(uid)
    }
}

fn non_empty_access_token(response: HarnessTokenResponse) -> Result<String, HarnessRequestError> {
    let token = response.access_token.trim().to_string();
    if token.is_empty() {
        Err(HarnessRequestError::from_error(
            "harness response missing access_token",
        ))
    } else {
        Ok(token)
    }
}

struct HarnessCreatedAccessToken {
    identifier: String,
    access_token: String,
    resolved_harness_uid: String,
}

async fn create_harness_project_access_token(
    state: &AppState,
    base_url: &str,
    bearer_token: &str,
    identity: &HarnessProvisioningIdentity,
    user: &UserRecord,
) -> Result<HarnessCreatedAccessToken, HarnessRequestError> {
    let identifier = harness_project_pat_identifier(state, user);
    let body = HarnessCreateAccessTokenRequest {
        identifier: identifier.as_str(),
    };
    let endpoint = format!("{base_url}/api/v1/user/tokens");
    let response = harness_request_json::<HarnessTokenResponse, _>(
        state,
        Method::POST,
        endpoint.as_str(),
        Some(bearer_token),
        Some(&body),
    )
    .await?;
    let access_token = non_empty_access_token(HarnessTokenResponse {
        access_token: response.access_token,
        token: None,
    })?;
    let identifier = response
        .token
        .map(|token| token.identifier)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(identifier);
    info!(
        user_id = user.id.as_str(),
        username = user.username.as_str(),
        harness_uid = identity.uid.as_str(),
        token_identifier = identifier.as_str(),
        "harness project access token created"
    );
    Ok(HarnessCreatedAccessToken {
        identifier,
        access_token,
        resolved_harness_uid: identity.uid.clone(),
    })
}

async fn ensure_harness_root_space(
    state: &AppState,
    base_url: &str,
    access_token: &str,
    identity: &HarnessProvisioningIdentity,
    username: &str,
) -> Result<(), HarnessRequestError> {
    let description = format!("Chatos workspace for {username}");
    let space_body = HarnessCreateSpaceRequest {
        identifier: identity.space_identifier.as_str(),
        parent_ref: "",
        description: description.as_str(),
        is_public: false,
    };
    let endpoint = format!("{base_url}/api/v1/spaces");
    match harness_request_json::<Value, _>(
        state,
        Method::POST,
        endpoint.as_str(),
        Some(access_token),
        Some(&space_body),
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(err) if err.is_already_exists() => {
            ensure_harness_space_access(
                state,
                base_url,
                access_token,
                identity.space_identifier.as_str(),
            )
            .await
        }
        Err(err) => Err(err),
    }
}

async fn ensure_harness_space_access(
    state: &AppState,
    base_url: &str,
    access_token: &str,
    space_identifier: &str,
) -> Result<(), HarnessRequestError> {
    let endpoint = format!(
        "{base_url}/api/v1/spaces/{}",
        urlencoding::encode(space_identifier)
    );
    let _: Value = harness_request_json::<Value, ()>(
        state,
        Method::GET,
        endpoint.as_str(),
        Some(access_token),
        None,
    )
    .await?;
    Ok(())
}

async fn harness_request_json<TResp, TBody>(
    state: &AppState,
    method: Method,
    endpoint: &str,
    bearer_token: Option<&str>,
    body: Option<&TBody>,
) -> Result<TResp, HarnessRequestError>
where
    TResp: serde::de::DeserializeOwned,
    TBody: Serialize + ?Sized,
{
    let client = build_client_with_timeout(state.config.harness_request_timeout_ms)
        .map_err(HarnessRequestError::from_error)?;
    let mut request = client.request(method, endpoint);
    if let Some(token) = bearer_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(body) = body {
        request = request.json(body);
    }

    let response = request
        .send()
        .await
        .map_err(|err| HarnessRequestError::from_error(err.to_string()))?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(HarnessRequestError {
            status: Some(status),
            message: extract_error_message(body_text.as_str()),
        });
    }
    serde_json::from_str::<TResp>(body_text.as_str()).map_err(|err| {
        HarnessRequestError::from_error(format!("decode harness response failed: {err}"))
    })
}
