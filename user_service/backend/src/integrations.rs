// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::{Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use tracing::{info, warn};

use crate::models::{
    HarnessProvisioningRecord, UserModelConfigRecord, UserModelSettingsRecord, UserRecord,
    HARNESS_PROVISIONING_STATUS_FAILED, HARNESS_PROVISIONING_STATUS_PENDING,
    HARNESS_PROVISIONING_STATUS_PROVISIONED,
};
use crate::secrets::{decrypt_secret, encrypt_secret};
use crate::state::AppState;
use crate::store::now_rfc3339;

const HARNESS_MAX_IDENTIFIER_LEN: usize = 100;
const HARNESS_MAX_EMAIL_LEN: usize = 250;

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

#[derive(Debug, Serialize)]
struct HarnessCreateRepoRequest<'a> {
    parent_ref: &'a str,
    identifier: &'a str,
    default_branch: &'a str,
    description: &'a str,
    is_public: bool,
    readme: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessProjectRepoCreateRequest {
    pub project_id: String,
    pub project_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessProjectRepoResponse {
    pub space_identifier: String,
    pub repo_identifier: String,
    pub repo_path: String,
    pub git_url: String,
    pub git_ssh_url: Option<String>,
    pub default_branch: String,
    pub push_username: String,
    pub push_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessApiAccessResponse {
    pub base_url: String,
    pub access_token: String,
    pub harness_uid: String,
    pub space_identifier: String,
}

#[derive(Debug, Deserialize)]
struct HarnessRepositoryOutput {
    identifier: String,
    path: String,
    #[serde(default)]
    git_url: String,
    #[serde(default)]
    git_ssh_url: Option<String>,
    #[serde(default)]
    default_branch: Option<String>,
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

pub async fn create_harness_project_repo(
    state: &AppState,
    owner_user_id: &str,
    input: HarnessProjectRepoCreateRequest,
) -> Result<HarnessProjectRepoResponse, String> {
    if !state.config.harness_provisioning_enabled {
        return Err("harness provisioning is disabled".to_string());
    }
    let base_url = normalized_url(state.config.harness_base_url.as_deref())
        .ok_or_else(|| "HARNESS_BASE_URL is not configured".to_string())?;
    let record = state
        .store
        .find_harness_provisioning_by_user_id(owner_user_id)
        .await?
        .ok_or_else(|| "harness provisioning record not found".to_string())?;
    if record.status != HARNESS_PROVISIONING_STATUS_PROVISIONED {
        return Err(format!(
            "harness provisioning is not ready: {}",
            record.status
        ));
    }
    let encrypted_access_token = record
        .encrypted_access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "harness access token is unavailable; login again or retry provisioning".to_string()
        })?;
    let push_token = decrypt_secret(encrypted_access_token)?;
    let repo_identifier =
        harness_repo_identifier(input.project_name.as_str(), input.project_id.as_str());
    let description = input
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Chatos cloud project");
    let body = HarnessCreateRepoRequest {
        parent_ref: record.space_identifier.as_str(),
        identifier: repo_identifier.as_str(),
        default_branch: "main",
        description,
        is_public: false,
        readme: false,
    };
    let endpoint = format!("{base_url}/api/v1/repos");
    let repo = harness_request_json::<HarnessRepositoryOutput, _>(
        state,
        Method::POST,
        endpoint.as_str(),
        Some(push_token.as_str()),
        Some(&body),
    )
    .await
    .map_err(|err| format!("create harness repo failed: {err}"))?;
    let git_url = rewrite_harness_local_url_host(repo.git_url.as_str(), &base_url, true);
    if git_url.is_empty() {
        return Err("harness repo response missing git_url".to_string());
    }
    Ok(HarnessProjectRepoResponse {
        space_identifier: record.space_identifier,
        repo_identifier: repo.identifier,
        repo_path: repo.path,
        git_url,
        git_ssh_url: repo
            .git_ssh_url
            .map(|value| rewrite_harness_local_url_host(value.as_str(), &base_url, false))
            .filter(|value| !value.is_empty()),
        default_branch: repo
            .default_branch
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "main".to_string()),
        push_username: record.harness_uid,
        push_token,
    })
}

pub async fn get_harness_api_access_for_user(
    state: &AppState,
    owner_user_id: &str,
) -> Result<HarnessApiAccessResponse, String> {
    if !state.config.harness_provisioning_enabled {
        return Err("harness provisioning is disabled".to_string());
    }
    let base_url = normalized_url(state.config.harness_base_url.as_deref())
        .ok_or_else(|| "HARNESS_BASE_URL is not configured".to_string())?;
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err("owner_user_id is required".to_string());
    }
    let record = state
        .store
        .find_harness_provisioning_by_user_id(owner_user_id)
        .await?
        .ok_or_else(|| "harness provisioning record not found".to_string())?;
    if record.status != HARNESS_PROVISIONING_STATUS_PROVISIONED {
        return Err(format!(
            "harness provisioning is not ready: {}",
            record.status
        ));
    }
    let encrypted_access_token = record
        .encrypted_access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "harness access token is unavailable; login again or retry provisioning".to_string()
        })?;
    Ok(HarnessApiAccessResponse {
        base_url,
        access_token: decrypt_secret(encrypted_access_token)?,
        harness_uid: record.harness_uid,
        space_identifier: record.space_identifier,
    })
}

fn rewrite_harness_local_url_host(
    raw_url: &str,
    harness_base_url: &str,
    rewrite_origin: bool,
) -> String {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Ok(mut url) = Url::parse(trimmed) else {
        return trimmed.to_string();
    };
    let Some(current_host) = url.host_str() else {
        return trimmed.to_string();
    };
    if !is_local_harness_host(current_host) {
        return trimmed.to_string();
    }

    let Ok(base_url) = Url::parse(harness_base_url) else {
        return trimmed.to_string();
    };
    let Some(base_host) = base_url.host_str() else {
        return trimmed.to_string();
    };

    if rewrite_origin {
        let _ = url.set_scheme(base_url.scheme());
        let _ = url.set_port(base_url.port());
    }
    let _ = url.set_host(Some(base_host));
    url.to_string()
}

fn is_local_harness_host(host: &str) -> bool {
    let normalized = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    normalized == "localhost"
        || normalized == "::1"
        || normalized == "0.0.0.0"
        || normalized.starts_with("127.")
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

fn harness_uid_for_user(user: &UserRecord) -> String {
    let username = user.username.trim().to_ascii_lowercase();
    if is_harness_identifier(username.as_str()) && !username.eq_ignore_ascii_case("anonymous") {
        return username;
    }
    format!("chatos-{}", compact_user_id(user.id.as_str()))
}

fn harness_project_pat_identifier(state: &AppState, user: &UserRecord) -> String {
    let prefix = sanitize_harness_identifier_part(
        state.config.harness_project_pat_prefix.as_str(),
        "chatos-project-import",
    );
    let user_part = sanitize_harness_identifier_part(user.username.as_str(), "user");
    let suffix = compact_user_id(uuid::Uuid::new_v4().to_string().as_str());
    truncate_harness_identifier(format!("{prefix}-{user_part}-{suffix}").as_str())
}

fn harness_repo_identifier(project_name: &str, project_id: &str) -> String {
    let name = sanitize_harness_identifier_part(project_name, "project");
    let suffix = compact_user_id(project_id);
    truncate_harness_identifier(format!("{name}-{suffix}").as_str())
}

fn harness_email_for_user(user: &UserRecord, uid: &str, synthetic_email_domain: &str) -> String {
    let username = user.username.trim();
    if username.contains('@') && !username.is_empty() && username.len() <= HARNESS_MAX_EMAIL_LEN {
        return username.to_ascii_lowercase();
    }

    let domain = synthetic_email_domain
        .trim()
        .trim_start_matches('@')
        .trim_matches('.')
        .to_ascii_lowercase();
    let domain = if domain.is_empty() {
        "chatos.local".to_string()
    } else {
        domain
    };
    let email = format!("{uid}@{domain}");
    if email.len() <= HARNESS_MAX_EMAIL_LEN {
        email
    } else {
        format!("{}@chatos.local", compact_user_id(user.id.as_str()))
    }
}

fn harness_space_identifier_for_user(
    user: &UserRecord,
    uid: &str,
    harness_space_prefix: &str,
) -> String {
    let prefix = harness_space_prefix.trim();
    let prefix = if prefix.is_empty() { "u-" } else { prefix };
    let candidate = format!("{prefix}{uid}");
    if is_valid_root_space_identifier(candidate.as_str()) {
        return candidate;
    }

    let fallback = format!("u-{}", compact_user_id(user.id.as_str()));
    if is_valid_root_space_identifier(fallback.as_str()) {
        fallback
    } else {
        "u-chatos-user".to_string()
    }
}

fn compact_user_id(user_id: &str) -> String {
    let compact: String = user_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(12)
        .collect();
    if compact.is_empty() {
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .take(12)
            .collect()
    } else {
        compact.to_ascii_lowercase()
    }
}

fn sanitize_harness_identifier_part(value: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_ascii_lowercase().chars() {
        let next = if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.') {
            ch
        } else if ch == '-' || ch.is_whitespace() {
            '-'
        } else {
            '-'
        };
        if next == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        out.push(next);
    }
    let out = out.trim_matches('-').trim_matches('.').to_string();
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

fn truncate_harness_identifier(value: &str) -> String {
    let trimmed = value.trim().trim_matches('-').trim_matches('.');
    if trimmed.len() <= HARNESS_MAX_IDENTIFIER_LEN {
        return trimmed.to_string();
    }
    trimmed
        .chars()
        .take(HARNESS_MAX_IDENTIFIER_LEN)
        .collect::<String>()
        .trim_matches('-')
        .trim_matches('.')
        .to_string()
}

fn truncate_error(error: &str) -> String {
    const MAX_ERROR_LEN: usize = 1000;
    let trimmed = error.trim();
    if trimmed.len() <= MAX_ERROR_LEN {
        trimmed.to_string()
    } else {
        trimmed.chars().take(MAX_ERROR_LEN).collect()
    }
}

fn is_harness_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= HARNESS_MAX_IDENTIFIER_LEN
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn is_valid_root_space_identifier(value: &str) -> bool {
    if !is_harness_identifier(value) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if lower == "api" || lower == "git" || lower.ends_with(".git") {
        return false;
    }
    !value.chars().all(|ch| ch.is_ascii_digit())
}

pub async fn sync_model_config_upsert(
    state: &AppState,
    config: &UserModelConfigRecord,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Err(err) = sync_memory_engine_model_profile(state, config).await {
        warn!(
            model_config_id = config.id.as_str(),
            owner_user_id = config.owner_user_id.as_str(),
            error = err.as_str(),
            "sync model config to memory_engine failed"
        );
        warnings.push(format!("memory_engine model update failed: {err}"));
    }

    if let Err(err) = sync_task_runner_model_config(state, config).await {
        warn!(
            model_config_id = config.id.as_str(),
            owner_user_id = config.owner_user_id.as_str(),
            error = err.as_str(),
            "sync model config to task_runner failed"
        );
        warnings.push(format!("task_runner model update failed: {err}"));
    }

    warnings
}

pub async fn sync_model_config_delete(state: &AppState, model_config_id: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Err(err) = delete_memory_engine_model_profile(state, model_config_id).await {
        warn!(
            model_config_id,
            error = err.as_str(),
            "delete memory_engine model profile failed"
        );
        warnings.push(format!("memory_engine delete failed: {err}"));
    }

    if let Err(err) = delete_task_runner_model_config(state, model_config_id).await {
        warn!(
            model_config_id,
            error = err.as_str(),
            "delete task_runner model config failed"
        );
        warnings.push(format!("task_runner delete failed: {err}"));
    }

    warnings
}

pub async fn sync_model_settings(
    state: &AppState,
    settings: &UserModelSettingsRecord,
) -> Vec<String> {
    let Some(memory_engine_base_url) =
        normalized_url(state.config.memory_engine_base_url.as_deref())
    else {
        return Vec::new();
    };
    let Some(operator_token) =
        normalized_text(state.config.memory_engine_operator_token.as_deref())
    else {
        return vec!["memory_engine operator token is not configured".to_string()];
    };

    let mut warnings = Vec::new();
    let owner_user_id = settings.user_id.as_str();
    let profiles = match list_memory_engine_model_profiles(
        state,
        memory_engine_base_url.as_str(),
        operator_token.as_str(),
        owner_user_id,
    )
    .await
    {
        Ok(items) => items,
        Err(err) => {
            warn!(
                owner_user_id,
                error = err.as_str(),
                "load memory_engine model profiles for settings update failed"
            );
            return vec![format!("memory_engine settings update failed: {err}")];
        }
    };

    let selected_id = normalized_text(settings.memory_summary_model_config_id.as_deref());
    for profile in profiles {
        let profile_id = profile
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        if profile_id.is_empty() {
            continue;
        }
        let desired_default = selected_id.as_deref() == Some(profile_id.as_str());
        let current_default = profile
            .get("is_default")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let current_thinking_level = profile
            .get("thinking_level")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let desired_thinking_level = if desired_default {
            normalized_text(settings.memory_summary_thinking_level.as_deref())
        } else {
            current_thinking_level.map(ToOwned::to_owned)
        };
        if current_default == desired_default
            && current_thinking_level == desired_thinking_level.as_deref()
        {
            continue;
        }

        let body = serde_json::json!({
            "id": profile_id,
            "name": profile.get("name").and_then(Value::as_str),
            "provider": profile.get("provider").and_then(Value::as_str),
            "model": profile.get("model").and_then(Value::as_str),
            "base_url": profile.get("base_url"),
            "api_key": profile.get("api_key"),
            "supports_images": profile.get("supports_images"),
            "supports_reasoning": profile.get("supports_reasoning"),
            "supports_responses": profile.get("supports_responses"),
            "temperature": profile.get("temperature"),
            "thinking_level": desired_thinking_level,
            "is_default": desired_default,
            "enabled": profile.get("enabled"),
        });

        if let Err(err) = memory_engine_request_json::<Value, _>(
            state,
            Method::PUT,
            &format!(
                "{memory_engine_base_url}/admin/model-profiles/{}",
                urlencoding::encode(profile_id.as_str())
            ),
            operator_token.as_str(),
            Some(&body),
        )
        .await
        {
            warn!(
                owner_user_id,
                model_config_id = profile_id.as_str(),
                error = err.as_str(),
                "update memory_engine profile default flag failed"
            );
            warnings.push(format!(
                "memory_engine default model update failed for {}: {err}",
                profile_id
            ));
        }
    }

    warnings
}

async fn sync_memory_engine_model_profile(
    state: &AppState,
    config: &UserModelConfigRecord,
) -> Result<(), String> {
    ensure_concrete_model(config)?;
    let Some(memory_engine_base_url) =
        normalized_url(state.config.memory_engine_base_url.as_deref())
    else {
        return Ok(());
    };
    let Some(operator_token) =
        normalized_text(state.config.memory_engine_operator_token.as_deref())
    else {
        return Err("MEMORY_ENGINE_OPERATOR_TOKEN is not configured".to_string());
    };

    let settings = state
        .store
        .get_user_model_settings(config.owner_user_id.as_str())
        .await?;
    let is_default = settings
        .as_ref()
        .and_then(|settings| settings.memory_summary_model_config_id.as_deref())
        .is_some_and(|value| value == config.id);
    let thinking_level = if is_default {
        settings
            .as_ref()
            .and_then(|settings| settings.memory_summary_thinking_level.clone())
    } else {
        config.thinking_level.clone()
    };

    let payload = serde_json::json!({
        "id": config.id,
        "name": config.name,
        "provider": memory_engine_provider(config.provider.as_str()),
        "model": config.model,
        "base_url": config.base_url,
        "api_key": config.api_key,
        "supports_images": config.supports_images,
        "supports_reasoning": config.supports_reasoning,
        "supports_responses": config.supports_responses,
        "temperature": Value::Null,
        "thinking_level": thinking_level,
        "is_default": is_default,
        "enabled": config.enabled,
    });

    let get_url = format!(
        "{memory_engine_base_url}/admin/model-profiles/{}",
        urlencoding::encode(config.id.as_str())
    );
    let exists = memory_engine_request_json::<Value, _>(
        state,
        Method::GET,
        get_url.as_str(),
        operator_token.as_str(),
        Option::<&()>::None,
    )
    .await
    .is_ok();

    let request_url = if exists {
        get_url
    } else {
        format!(
            "{memory_engine_base_url}/admin/model-profiles?owner_user_id={}",
            urlencoding::encode(config.owner_user_id.as_str())
        )
    };
    let method = if exists { Method::PUT } else { Method::POST };

    let _: Value = memory_engine_request_json(
        state,
        method,
        request_url.as_str(),
        operator_token.as_str(),
        Some(&payload),
    )
    .await?;
    Ok(())
}

async fn delete_memory_engine_model_profile(
    state: &AppState,
    model_config_id: &str,
) -> Result<(), String> {
    let Some(memory_engine_base_url) =
        normalized_url(state.config.memory_engine_base_url.as_deref())
    else {
        return Ok(());
    };
    let Some(operator_token) =
        normalized_text(state.config.memory_engine_operator_token.as_deref())
    else {
        return Err("MEMORY_ENGINE_OPERATOR_TOKEN is not configured".to_string());
    };

    let endpoint = format!(
        "{memory_engine_base_url}/admin/model-profiles/{}",
        urlencoding::encode(model_config_id)
    );
    let response = build_client(state)?
        .request(Method::DELETE, endpoint)
        .header("x-memory-operator-token", operator_token.trim())
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if status.is_success() || status.as_u16() == 404 {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "memory_engine delete request failed: {} {}",
        status.as_u16(),
        extract_error_message(body.as_str())
    ))
}

async fn list_memory_engine_model_profiles(
    state: &AppState,
    base_url: &str,
    operator_token: &str,
    owner_user_id: &str,
) -> Result<Vec<Value>, String> {
    let endpoint = format!(
        "{base_url}/admin/model-profiles?owner_user_id={}",
        urlencoding::encode(owner_user_id)
    );
    let payload: Value = memory_engine_request_json(
        state,
        Method::GET,
        endpoint.as_str(),
        operator_token,
        Option::<&()>::None,
    )
    .await?;
    Ok(payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

async fn sync_task_runner_model_config(
    state: &AppState,
    config: &UserModelConfigRecord,
) -> Result<(), String> {
    ensure_concrete_model(config)?;
    let Some(task_runner_base_url) = normalized_url(state.config.task_runner_base_url.as_deref())
    else {
        return Ok(());
    };

    let payload = serde_json::json!({
        "id": config.id,
        "owner_user_id": config.owner_user_id,
        "name": config.name,
        "provider": task_runner_provider(config.provider.as_str()),
        "base_url": config.base_url,
        "api_key": config.api_key,
        "model": config.model,
        "usage_scenario": config.task_usage_scenario,
        "thinking_level": config.task_thinking_level,
        "supports_responses": config.supports_responses,
        "enabled": config.enabled,
    });

    let _: Value = task_runner_request_json(
        state,
        Method::POST,
        &format!("{task_runner_base_url}/api/chatos-sync/model-configs"),
        Some(&payload),
    )
    .await?;
    Ok(())
}

async fn delete_task_runner_model_config(
    state: &AppState,
    model_config_id: &str,
) -> Result<(), String> {
    let Some(task_runner_base_url) = normalized_url(state.config.task_runner_base_url.as_deref())
    else {
        return Ok(());
    };
    let endpoint = format!(
        "{task_runner_base_url}/api/chatos-sync/model-configs/{}",
        urlencoding::encode(model_config_id)
    );
    let response = task_runner_request(state, Method::DELETE, endpoint.as_str())?
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if response.status().is_success() || response.status().as_u16() == 404 {
        return Ok(());
    }
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Err(format!(
        "task_runner delete request failed: {} {}",
        status,
        extract_error_message(body.as_str())
    ))
}

fn build_client(state: &AppState) -> Result<reqwest::Client, String> {
    build_client_with_timeout(state.config.downstream_request_timeout_ms)
}

fn build_client_with_timeout(timeout_ms: i64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms.max(300) as u64))
        .build()
        .map_err(|err| err.to_string())
}

async fn memory_engine_request_json<TResp, TBody>(
    state: &AppState,
    method: Method,
    endpoint: &str,
    operator_token: &str,
    body: Option<&TBody>,
) -> Result<TResp, String>
where
    TResp: serde::de::DeserializeOwned,
    TBody: Serialize + ?Sized,
{
    let client = build_client(state)?;
    let mut request = client
        .request(method, endpoint)
        .header("x-memory-operator-token", operator_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory_engine request failed: {} {}",
            status.as_u16(),
            extract_error_message(body.as_str())
        ));
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| err.to_string())
}

fn task_runner_request(
    state: &AppState,
    method: Method,
    endpoint: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let mut request = build_client(state)?.request(method, endpoint);
    if let Some(secret) = normalized_text(state.config.task_runner_callback_secret.as_deref()) {
        request = request.header("x-chatos-callback-secret", secret);
    }
    Ok(request)
}

async fn task_runner_request_json<TResp, TBody>(
    state: &AppState,
    method: Method,
    endpoint: &str,
    body: Option<&TBody>,
) -> Result<TResp, String>
where
    TResp: serde::de::DeserializeOwned,
    TBody: Serialize + ?Sized,
{
    let mut request = task_runner_request(state, method, endpoint)?;
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "task_runner request failed: {} {}",
            status.as_u16(),
            extract_error_message(body.as_str())
        ));
    }
    response
        .json::<TResp>()
        .await
        .map_err(|err| err.to_string())
}

fn task_runner_provider(provider: &str) -> &'static str {
    match provider.trim() {
        "deepseek" => "deepseek",
        "kimi" => "kimik2",
        "openai_compatible" | "minimax" => "openai_compatible",
        _ => "openai",
    }
}

fn memory_engine_provider(provider: &str) -> &'static str {
    match provider.trim() {
        "deepseek" => "deepseek",
        "kimi" => "openai",
        "minimax" => "openai",
        "openai_compatible" => "openai",
        _ => "openai",
    }
}

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_url(value: Option<&str>) -> Option<String> {
    normalized_text(value).map(|value| value.trim_end_matches('/').to_string())
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_string())
}

fn ensure_concrete_model(config: &UserModelConfigRecord) -> Result<(), String> {
    if config.model.trim().is_empty() {
        return Err("model is empty; downstream services require a concrete model".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        harness_email_for_user, harness_space_identifier_for_user, harness_uid_for_user,
        is_valid_root_space_identifier, rewrite_harness_local_url_host,
    };
    use crate::models::{UserRecord, USER_ROLE_USER};

    fn test_user(username: &str) -> UserRecord {
        UserRecord {
            id: "12345678-90ab-cdef-1234-567890abcdef".to_string(),
            username: username.to_string(),
            display_name: username.to_string(),
            password_hash: "hash".to_string(),
            role: USER_ROLE_USER.to_string(),
            enabled: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            last_login_at: None,
        }
    }

    #[test]
    fn harness_uid_reuses_valid_username() {
        let user = test_user("leeoohoo");
        assert_eq!(harness_uid_for_user(&user), "leeoohoo");
    }

    #[test]
    fn harness_uid_falls_back_for_email_username() {
        let user = test_user("alice@example.com");
        assert_eq!(harness_uid_for_user(&user), "chatos-1234567890ab");
        assert_eq!(
            harness_email_for_user(&user, "chatos-1234567890ab", "chatos.local"),
            "alice@example.com"
        );
    }

    #[test]
    fn harness_email_uses_synthetic_domain_for_plain_username() {
        let user = test_user("leeoohoo");
        assert_eq!(
            harness_email_for_user(&user, "leeoohoo", "@example.internal."),
            "leeoohoo@example.internal"
        );
    }

    #[test]
    fn harness_space_identifier_uses_prefix_and_fallback() {
        let user = test_user("leeoohoo");
        assert_eq!(
            harness_space_identifier_for_user(&user, "leeoohoo", "u-"),
            "u-leeoohoo"
        );
        assert_eq!(
            harness_space_identifier_for_user(&user, "leeoohoo", "bad@"),
            "u-1234567890ab"
        );
    }

    #[test]
    fn root_space_identifier_rejects_harness_reserved_values() {
        assert!(!is_valid_root_space_identifier("12345"));
        assert!(!is_valid_root_space_identifier("api"));
        assert!(!is_valid_root_space_identifier("git"));
        assert!(!is_valid_root_space_identifier("project.git"));
        assert!(is_valid_root_space_identifier("u-leeoohoo"));
    }

    #[test]
    fn harness_repo_git_url_rewrites_localhost_to_configured_base_url() {
        assert_eq!(
            rewrite_harness_local_url_host(
                "http://localhost:3000/git/u-leeoohoo/project.git",
                "http://8.155.171.124:3000",
                true,
            ),
            "http://8.155.171.124:3000/git/u-leeoohoo/project.git"
        );
        assert_eq!(
            rewrite_harness_local_url_host(
                "ssh://git@localhost:3022/u-leeoohoo/project.git",
                "http://8.155.171.124:3000",
                false,
            ),
            "ssh://git@8.155.171.124:3022/u-leeoohoo/project.git"
        );
    }

    #[test]
    fn harness_repo_git_url_keeps_non_local_hosts() {
        assert_eq!(
            rewrite_harness_local_url_host(
                "https://git.example.com/u-leeoohoo/project.git",
                "http://8.155.171.124:3000",
                true,
            ),
            "https://git.example.com/u-leeoohoo/project.git"
        );
    }
}
