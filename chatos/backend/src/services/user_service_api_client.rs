// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;
use serde::Serialize;

mod http;
mod types;

use http::{request_empty, request_json};
pub use types::{
    CreateUserServiceAgentAccountRequest, CreateUserServiceModelConfigRequest,
    CreateUserServiceModelProviderRequest, DeviceProofVerificationRequest,
    UpdateUserServiceModelConfigRequest, UpdateUserServiceModelProviderRequest,
    UpdateUserServiceModelSettingsRequest, UserServiceAgentAccountSummary, UserServiceAuthUser,
    UserServiceInternalModelRuntimeRecord, UserServiceLocalConnectorTicketResponse,
    UserServiceLoginResponse, UserServiceMeResponse, UserServiceModelConfigRecord,
    UserServiceModelProviderRecord, UserServiceModelSettingsRecord, UserServiceVerifiedPrincipal,
    UserServiceVerifyResponse,
};

const CHATOS_INTERNAL_CALLER: &str = "chatos-backend";
const USER_SERVICE_INTERNAL_AUDIENCE: &str = "user-service";
const MODEL_RUNTIME_READ_SCOPE: &str = "model-runtime.read";
const MODEL_SETTINGS_READ_SCOPE: &str = "model-settings.read";

pub fn response_status_from_error(error: &str) -> Option<u16> {
    error
        .trim()
        .strip_prefix("user_service request failed:")?
        .split_whitespace()
        .next()?
        .parse::<u16>()
        .ok()
}

#[derive(Debug, Serialize)]
struct UserServiceAuthRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Debug, Serialize)]
struct UserServiceRegisterRequest<'a> {
    email: &'a str,
    username: &'a str,
    password: &'a str,
    invite_code: &'a str,
    verification_code: &'a str,
}

#[derive(Debug, Serialize)]
struct UserServiceSendRegisterCodeRequest<'a> {
    email: &'a str,
    invite_code: &'a str,
}

pub async fn login(
    base_url: &str,
    username: &str,
    password: &str,
    timeout_ms: i64,
) -> Result<UserServiceLoginResponse, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/auth/login",
        None,
        Some(&UserServiceAuthRequest { username, password }),
        timeout_ms,
    )
    .await
}

pub async fn register(
    base_url: &str,
    email: &str,
    password: &str,
    invite_code: &str,
    verification_code: &str,
    timeout_ms: i64,
) -> Result<UserServiceLoginResponse, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/auth/register",
        None,
        Some(&UserServiceRegisterRequest {
            email,
            username: email,
            password,
            invite_code,
            verification_code,
        }),
        timeout_ms,
    )
    .await
}

pub async fn send_register_email_code(
    base_url: &str,
    email: &str,
    invite_code: &str,
    timeout_ms: i64,
) -> Result<serde_json::Value, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/auth/register/send-code",
        None,
        Some(&UserServiceSendRegisterCodeRequest { email, invite_code }),
        timeout_ms,
    )
    .await
}

pub async fn issue_local_connector_ticket(
    base_url: &str,
    access_token: &str,
    timeout_ms: i64,
) -> Result<UserServiceLocalConnectorTicketResponse, String> {
    request_json::<(), _>(
        Method::POST,
        base_url,
        "/api/auth/local-connector-ticket",
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn get_me(
    base_url: &str,
    access_token: &str,
    timeout_ms: i64,
) -> Result<UserServiceMeResponse, String> {
    request_json::<(), _>(
        Method::GET,
        base_url,
        "/api/auth/me",
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn verify_token(
    base_url: &str,
    access_token: &str,
    timeout_ms: i64,
) -> Result<UserServiceVerifyResponse, String> {
    request_json::<(), _>(
        Method::GET,
        base_url,
        "/api/auth/verify",
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn verify_device_request(
    base_url: &str,
    access_token: &str,
    proof: &DeviceProofVerificationRequest,
    timeout_ms: i64,
) -> Result<UserServiceVerifyResponse, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/auth/device-proof/verify",
        Some(access_token),
        Some(proof),
        timeout_ms,
    )
    .await
}

pub async fn list_agent_accounts(
    base_url: &str,
    access_token: &str,
    timeout_ms: i64,
) -> Result<Vec<UserServiceAgentAccountSummary>, String> {
    request_json::<(), _>(
        Method::GET,
        base_url,
        "/api/agent-accounts",
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn create_agent_account(
    base_url: &str,
    access_token: &str,
    payload: &CreateUserServiceAgentAccountRequest,
    timeout_ms: i64,
) -> Result<UserServiceAgentAccountSummary, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/agent-accounts",
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn list_model_configs(
    base_url: &str,
    access_token: &str,
    user_id: Option<&str>,
    timeout_ms: i64,
) -> Result<Vec<UserServiceModelConfigRecord>, String> {
    let path = match user_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(user_id) => format!(
            "/api/model-configs?user_id={}",
            urlencoding::encode(user_id)
        ),
        None => "/api/model-configs".to_string(),
    };
    request_json::<(), _>(
        Method::GET,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn list_model_providers(
    base_url: &str,
    access_token: &str,
    user_id: Option<&str>,
    timeout_ms: i64,
) -> Result<Vec<UserServiceModelProviderRecord>, String> {
    let path = match user_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(user_id) => format!(
            "/api/model-providers?user_id={}",
            urlencoding::encode(user_id)
        ),
        None => "/api/model-providers".to_string(),
    };
    request_json::<(), _>(
        Method::GET,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn get_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    include_secret: bool,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    let path = if include_secret {
        format!(
            "/api/model-configs/{}?include_secret=true",
            urlencoding::encode(id.trim())
        )
    } else {
        format!("/api/model-configs/{}", urlencoding::encode(id.trim()))
    };
    request_json::<(), _>(
        Method::GET,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn get_internal_model_runtime_config(
    client: &reqwest::Client,
    base_url: &str,
    internal_secret: &str,
    user_id: &str,
    model_config_id: &str,
) -> Result<UserServiceInternalModelRuntimeRecord, String> {
    let internal_secret = internal_secret.trim();
    if internal_secret.is_empty() {
        return Err("chatos user service internal secret is required".to_string());
    }
    let token = chatos_service_runtime::issue_internal_service_token(
        internal_secret,
        CHATOS_INTERNAL_CALLER,
        USER_SERVICE_INTERNAL_AUDIENCE,
        MODEL_RUNTIME_READ_SCOPE,
        60,
    )?;
    let url = format!(
        "{}/api/internal/users/{}/model-configs/{}/runtime",
        base_url.trim_end_matches('/'),
        urlencoding::encode(user_id.trim()),
        urlencoding::encode(model_config_id.trim()),
    );
    let response = client
        .get(url)
        .header("X-User-Service-Caller", CHATOS_INTERNAL_CALLER)
        .header("X-User-Service-Internal-Token", token)
        .send()
        .await
        .map_err(|error| format!("user service internal request failed: {error}"))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("read user service internal response failed: {error}"))?;
    if !status.is_success() {
        let detail = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .or_else(|| value.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| String::from_utf8_lossy(&body).into_owned());
        return Err(format!(
            "user service internal request failed: {} {}",
            status.as_u16(),
            detail.trim()
        ));
    }
    serde_json::from_slice(&body)
        .map_err(|error| format!("decode user service internal model runtime failed: {error}"))
}

pub async fn list_internal_model_runtime_configs(
    client: &reqwest::Client,
    base_url: &str,
    internal_secret: &str,
    user_id: &str,
) -> Result<Vec<UserServiceInternalModelRuntimeRecord>, String> {
    let internal_secret = internal_secret.trim();
    if internal_secret.is_empty() {
        return Err("chatos user service internal secret is required".to_string());
    }
    let token = chatos_service_runtime::issue_internal_service_token(
        internal_secret,
        CHATOS_INTERNAL_CALLER,
        USER_SERVICE_INTERNAL_AUDIENCE,
        MODEL_RUNTIME_READ_SCOPE,
        60,
    )?;
    let url = format!(
        "{}/api/internal/users/{}/model-configs/runtime",
        base_url.trim_end_matches('/'),
        urlencoding::encode(user_id.trim()),
    );
    let response = client
        .get(url)
        .header("X-User-Service-Caller", CHATOS_INTERNAL_CALLER)
        .header("X-User-Service-Internal-Token", token)
        .send()
        .await
        .map_err(|error| format!("user service internal request failed: {error}"))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("read user service internal response failed: {error}"))?;
    if !status.is_success() {
        let detail = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .or_else(|| value.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| String::from_utf8_lossy(&body).into_owned());
        return Err(format!(
            "user service internal request failed: {} {}",
            status.as_u16(),
            detail.trim()
        ));
    }
    serde_json::from_slice(&body)
        .map_err(|error| format!("decode user service internal model runtime list failed: {error}"))
}

pub async fn get_internal_user_model_settings(
    client: &reqwest::Client,
    base_url: &str,
    internal_secret: &str,
    user_id: &str,
) -> Result<UserServiceModelSettingsRecord, String> {
    let internal_secret = internal_secret.trim();
    if internal_secret.is_empty() {
        return Err("chatos user service internal secret is required".to_string());
    }
    let token = chatos_service_runtime::issue_internal_service_token(
        internal_secret,
        CHATOS_INTERNAL_CALLER,
        USER_SERVICE_INTERNAL_AUDIENCE,
        MODEL_SETTINGS_READ_SCOPE,
        60,
    )?;
    let url = format!(
        "{}/api/internal/users/{}/model-settings",
        base_url.trim_end_matches('/'),
        urlencoding::encode(user_id.trim()),
    );
    let response = client
        .get(url)
        .header("X-User-Service-Caller", CHATOS_INTERNAL_CALLER)
        .header("X-User-Service-Internal-Token", token)
        .send()
        .await
        .map_err(|error| format!("user service internal request failed: {error}"))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("read user service internal response failed: {error}"))?;
    if !status.is_success() {
        let detail = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .or_else(|| value.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| String::from_utf8_lossy(&body).into_owned());
        return Err(format!(
            "user service internal request failed: {} {}",
            status.as_u16(),
            detail.trim()
        ));
    }
    serde_json::from_slice(&body)
        .map_err(|error| format!("decode user service internal model settings failed: {error}"))
}

pub async fn get_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    include_secret: bool,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    let path = if include_secret {
        format!(
            "/api/model-providers/{}?include_secret=true",
            urlencoding::encode(id.trim())
        )
    } else {
        format!("/api/model-providers/{}", urlencoding::encode(id.trim()))
    };
    request_json::<(), _>(
        Method::GET,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn get_model_settings(
    base_url: &str,
    access_token: &str,
    user_id: Option<&str>,
    timeout_ms: i64,
) -> Result<UserServiceModelSettingsRecord, String> {
    let path = match user_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(user_id) => format!(
            "/api/model-configs/settings?user_id={}",
            urlencoding::encode(user_id)
        ),
        None => "/api/model-configs/settings".to_string(),
    };
    request_json::<(), _>(
        Method::GET,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn update_model_settings(
    base_url: &str,
    access_token: &str,
    payload: &UpdateUserServiceModelSettingsRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelSettingsRecord, String> {
    request_json(
        Method::PUT,
        base_url,
        "/api/model-configs/settings",
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn create_model_config(
    base_url: &str,
    access_token: &str,
    payload: &CreateUserServiceModelConfigRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/model-configs",
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn create_model_provider(
    base_url: &str,
    access_token: &str,
    payload: &CreateUserServiceModelProviderRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    request_json(
        Method::POST,
        base_url,
        "/api/model-providers",
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn update_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelConfigRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    let path = format!("/api/model-configs/{}", urlencoding::encode(id.trim()));
    request_json(
        Method::PATCH,
        base_url,
        path.as_str(),
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn update_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelProviderRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    let path = format!("/api/model-providers/{}", urlencoding::encode(id.trim()));
    request_json(
        Method::PATCH,
        base_url,
        path.as_str(),
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn refresh_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelConfigRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelConfigRecord, String> {
    let path = format!(
        "/api/model-configs/{}/refresh",
        urlencoding::encode(id.trim())
    );
    request_json(
        Method::POST,
        base_url,
        path.as_str(),
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn refresh_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    payload: &UpdateUserServiceModelProviderRequest,
    timeout_ms: i64,
) -> Result<UserServiceModelProviderRecord, String> {
    let path = format!(
        "/api/model-providers/{}/refresh",
        urlencoding::encode(id.trim())
    );
    request_json(
        Method::POST,
        base_url,
        path.as_str(),
        Some(access_token),
        Some(payload),
        timeout_ms,
    )
    .await
}

pub async fn delete_model_config(
    base_url: &str,
    access_token: &str,
    id: &str,
    timeout_ms: i64,
) -> Result<(), String> {
    let path = format!("/api/model-configs/{}", urlencoding::encode(id.trim()));
    request_empty::<()>(
        Method::DELETE,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

pub async fn delete_model_provider(
    base_url: &str,
    access_token: &str,
    id: &str,
    timeout_ms: i64,
) -> Result<(), String> {
    let path = format!("/api/model-providers/{}", urlencoding::encode(id.trim()));
    request_empty::<()>(
        Method::DELETE,
        base_url,
        path.as_str(),
        Some(access_token),
        None,
        timeout_ms,
    )
    .await
}

#[cfg(test)]
include!("user_service_api_client_inline_tests.rs");
