// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::config::{
    api_url, default_device_name, ensure_remote_url_allowed, normalize_optional, ClientConfig,
};
use crate::registration::{disconnect_device, ensure_success};
use crate::{tracing_stdout, AuthState, LocalRuntime};

use super::super::types::{
    DesktopTicketAuthRequest, LocalApiError, LocalAuthRequest, LoginResponse,
    SendRegisterEmailCodeRequest,
};
use super::helpers::normalize_required;
use super::status::status_payload;

pub(crate) async fn local_login(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<LocalAuthRequest>,
) -> Result<Json<Value>, LocalApiError> {
    local_auth(runtime, req, false).await
}

pub(crate) async fn local_register(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<LocalAuthRequest>,
) -> Result<Json<Value>, LocalApiError> {
    local_auth(runtime, req, true).await
}

pub(crate) async fn local_send_register_email_code(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<SendRegisterEmailCodeRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let cloud_base_url = normalize_required(req.cloud_base_url.as_str(), "cloud_base_url")?;
    ensure_remote_url_allowed("cloud_base_url", cloud_base_url.as_str())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let email = normalize_required(req.email.as_str(), "email")?;
    let invite_code = normalize_required(req.invite_code.as_str(), "invite_code")?;
    let response = runtime
        .http_client
        .post(api_url(
            cloud_base_url.as_str(),
            "/api/auth/register/send-code",
        ))
        .json(&json!({
            "email": email,
            "invite_code": invite_code,
        }))
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let message = serde_json::from_str::<Value>(text.as_str())
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .or_else(|| value.get("detail").and_then(Value::as_str))
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| format!("send verification code failed with status {status}"));
        return Err(LocalApiError::bad_request(message));
    }
    let body = if text.trim().is_empty() {
        json!({ "ok": true })
    } else {
        serde_json::from_str::<Value>(text.as_str())
            .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?
    };
    Ok(Json(body))
}

pub(crate) async fn local_desktop_ticket(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<DesktopTicketAuthRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let cloud_base_url = normalize_required(req.cloud_base_url.as_str(), "cloud_base_url")?;
    ensure_remote_url_allowed("cloud_base_url", cloud_base_url.as_str())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let ticket = normalize_required(req.ticket.as_str(), "ticket")?;
    let response = runtime
        .http_client
        .post(api_url(
            cloud_base_url.as_str(),
            "/api/auth/local-connector-ticket/exchange",
        ))
        .json(&json!({
            "ticket": ticket,
            "device_name": normalize_optional(req.device_name.as_deref())
                .unwrap_or_else(default_device_name),
            "client_version": env!("CARGO_PKG_VERSION"),
        }))
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    ensure_success(response.status(), "exchange local connector ticket")
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let login = response
        .json::<LoginResponse>()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    apply_login(
        runtime,
        cloud_base_url,
        None,
        login,
        normalize_optional(req.device_name.as_deref()),
    )
    .await
}

async fn local_auth(
    runtime: LocalRuntime,
    req: LocalAuthRequest,
    register: bool,
) -> Result<Json<Value>, LocalApiError> {
    let cloud_base_url = normalize_required(req.cloud_base_url.as_str(), "cloud_base_url")?;
    let user_service_base_url = normalize_optional(req.user_service_base_url.as_deref())
        .unwrap_or_else(|| cloud_base_url.clone());
    ensure_remote_url_allowed("cloud_base_url", cloud_base_url.as_str())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    ensure_remote_url_allowed("user_service_base_url", user_service_base_url.as_str())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let username = normalize_required(req.username.as_str(), "username")?;
    let password = normalize_required(req.password.as_str(), "password")?;
    let endpoint = if register {
        "/api/auth/register"
    } else {
        "/api/auth/login"
    };
    let mut body = json!({
        "email": username,
        "username": username,
        "password": password,
    });
    if register {
        body["display_name"] = normalize_optional(req.display_name.as_deref())
            .map(Value::String)
            .unwrap_or(Value::Null);
        body["invite_code"] = normalize_optional(req.invite_code.as_deref())
            .map(Value::String)
            .unwrap_or(Value::Null);
        body["verification_code"] = normalize_optional(req.verification_code.as_deref())
            .map(Value::String)
            .unwrap_or(Value::Null);
    }
    let response = runtime
        .http_client
        .post(api_url(user_service_base_url.as_str(), endpoint).as_str())
        .json(&body)
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    ensure_success(response.status(), "authenticate user")
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let login = response
        .json::<LoginResponse>()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    apply_login(
        runtime,
        cloud_base_url,
        Some(user_service_base_url),
        login,
        normalize_optional(req.device_name.as_deref()),
    )
    .await
}

async fn apply_login(
    runtime: LocalRuntime,
    cloud_base_url: String,
    user_service_base_url: Option<String>,
    login: LoginResponse,
    device_name: Option<String>,
) -> Result<Json<Value>, LocalApiError> {
    let resolved_user_service_base_url =
        user_service_base_url.unwrap_or_else(|| cloud_base_url.clone());
    {
        let mut state = runtime.state.write().await;
        let pairing_changed = state.device_id.is_some()
            && !state.pairing_context_matches(cloud_base_url.as_str(), login.user.id.as_str());
        state.auth = Some(AuthState {
            cloud_base_url: cloud_base_url.clone(),
            user_service_base_url: resolved_user_service_base_url,
            access_token: login.token,
            device_name: device_name.unwrap_or_else(default_device_name),
            user: Some(login.user.clone()),
        });
        state.paired_cloud_base_url = Some(cloud_base_url);
        state.paired_user_id = Some(login.user.id);
        if pairing_changed {
            state.device_id = None;
            state.device_public_key = None;
        }
        state.save(runtime.state_path.as_path())?;
    }
    runtime.sync_saved_workspaces_if_needed().await?;
    runtime
        .reload_managed_requirements_for_current_identity()
        .await?;
    runtime.start_connector_if_configured().await?;
    Ok(Json(status_payload(&runtime).await))
}

pub(crate) async fn local_logout(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    let disconnect = {
        let state = runtime.state.read().await;
        ClientConfig::from_state(&state, runtime.state_path.clone()).zip(state.device_id.clone())
    };
    {
        let mut task = runtime.connector_task.lock().await;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }
    if let Some((config, device_id)) = disconnect {
        if let Err(err) = disconnect_device(&runtime.http_client, &config, device_id.as_str()).await
        {
            tracing_stdout(format!("mark local connector device offline failed: {err}").as_str());
        }
    }
    {
        let mut state = runtime.state.write().await;
        state.auth = None;
        state.sandbox.enabled = false;
        state.save(runtime.state_path.as_path())?;
    }
    runtime
        .reload_managed_requirements_for_current_identity()
        .await?;
    Ok(Json(status_payload(&runtime).await))
}
