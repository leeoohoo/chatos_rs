// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};

use crate::plugins::{
    ActivePluginInstallation, LocalPluginOAuthConnection, PluginCredentialMetadata,
    PluginCredentialScope, PluginOAuthAuthorizationStart,
};
use crate::runtime::runtime_identity;
use crate::{tracing_stdout, LocalRuntime};

use super::super::super::types::{
    BeginPluginOAuthRequest, CompletePluginOAuthRequest, LocalApiError,
    UpsertPluginCredentialRequest,
};

pub(crate) async fn local_plugin_credentials(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
) -> Result<Json<Vec<PluginCredentialMetadata>>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let installation = active_plugin_installation(&runtime, plugin_id.as_str()).await?;
    let release_id = installation.version.release_id;
    let vault = runtime.plugin_credentials.clone();
    let credentials = tokio::task::spawn_blocking(move || {
        vault.list(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
            release_id.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin credential list task failed: {error}"))??;
    Ok(Json(credentials))
}

pub(crate) async fn local_upsert_plugin_credential(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key, secret_name)): Path<(String, String, String)>,
    Json(request): Json<UpsertPluginCredentialRequest>,
) -> Result<Json<PluginCredentialMetadata>, LocalApiError> {
    if request.value.is_empty() || request.value.len() > 64 * 1024 {
        return Err(LocalApiError::bad_request(
            "Plugin credential must contain between 1 byte and 64 KiB",
        ));
    }
    let scope = active_credential_scope(&runtime, plugin_id, component_key, secret_name).await?;
    let vault = runtime.plugin_credentials.clone();
    let value = request.value.into_bytes();
    let metadata = tokio::task::spawn_blocking(move || {
        let mut value = value;
        let result = vault.upsert(&scope, value.as_slice());
        value.fill(0);
        result
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin credential write task failed: {error}"))??;
    Ok(Json(metadata))
}

pub(crate) async fn local_delete_plugin_credential(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key, secret_name)): Path<(String, String, String)>,
) -> Result<Json<Value>, LocalApiError> {
    let scope = active_credential_scope(&runtime, plugin_id, component_key, secret_name).await?;
    let vault = runtime.plugin_credentials.clone();
    let deleted = tokio::task::spawn_blocking(move || vault.delete(&scope))
        .await
        .map_err(|error| anyhow::anyhow!("join Plugin credential delete task failed: {error}"))??;
    Ok(Json(json!({ "deleted": deleted })))
}

pub(crate) async fn local_plugin_oauth_connections(
    State(runtime): State<LocalRuntime>,
    Path(plugin_id): Path<String>,
) -> Result<Json<Vec<LocalPluginOAuthConnection>>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let broker = runtime.plugin_oauth.clone();
    let connections = tokio::task::spawn_blocking(move || {
        broker.list_connections(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin OAuth list task failed: {error}"))??;
    Ok(Json(connections))
}

pub(crate) async fn local_begin_plugin_oauth(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key)): Path<(String, String)>,
    Json(request): Json<BeginPluginOAuthRequest>,
) -> Result<Json<PluginOAuthAuthorizationStart>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let installation = active_plugin_installation(&runtime, plugin_id.as_str()).await?;
    let release_id = installation.version.release_id;
    let broker = runtime.plugin_oauth.clone();
    let open_browser = request.open_browser.unwrap_or(true);
    let redirect_uri = super::super::super::local_plugin_oauth_redirect_uri();
    if request
        .redirect_uri
        .as_deref()
        .is_some_and(|requested| requested != redirect_uri)
    {
        return Err(LocalApiError::bad_request(
            "Plugin OAuth redirect_uri must match the Local Connector callback",
        ));
    }
    let mut authorization = tokio::task::spawn_blocking(move || {
        broker.begin_authorization(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
            redirect_uri.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin OAuth start task failed: {error}"))??;
    if open_browser {
        match crate::external_url::open_external_url(authorization.authorization_url.as_str()).await
        {
            Ok(()) => authorization.browser_opened = true,
            Err(error) => {
                tracing_stdout(
                    format!("open Plugin OAuth authorization URL failed: {error}").as_str(),
                );
                authorization.browser_error =
                    Some("The system browser could not be opened automatically".to_string());
            }
        }
    }
    Ok(Json(authorization))
}

pub(crate) async fn local_complete_plugin_oauth(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CompletePluginOAuthRequest>,
) -> Result<Json<LocalPluginOAuthConnection>, LocalApiError> {
    complete_plugin_oauth(runtime, request).await
}

pub(crate) async fn local_complete_plugin_oauth_query(
    State(runtime): State<LocalRuntime>,
    Query(request): Query<CompletePluginOAuthRequest>,
) -> Result<Json<LocalPluginOAuthConnection>, LocalApiError> {
    complete_plugin_oauth(runtime, request).await
}

pub(crate) async fn local_disconnect_plugin_oauth(
    State(runtime): State<LocalRuntime>,
    Path((plugin_id, component_key, provider)): Path<(String, String, String)>,
) -> Result<Json<Value>, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(&runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let broker = runtime.plugin_oauth.clone();
    let disconnected = tokio::task::spawn_blocking(move || {
        broker.disconnect(
            owner_user_id.as_str(),
            device_id.as_str(),
            plugin_id.as_str(),
            component_key.as_str(),
            provider.as_str(),
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("join Plugin OAuth disconnect task failed: {error}"))??;
    Ok(Json(json!({ "disconnected": disconnected })))
}

async fn active_credential_scope(
    runtime: &LocalRuntime,
    plugin_id: String,
    component_key: String,
    secret_name: String,
) -> Result<PluginCredentialScope, LocalApiError> {
    let (owner_user_id, device_id) = runtime_identity(runtime)
        .await
        .map_err(|error| LocalApiError::conflict(error.to_string()))?;
    let installation = active_plugin_installation(runtime, plugin_id.as_str()).await?;
    if !installation
        .version
        .inventory
        .components
        .iter()
        .any(|component| component.component_key == component_key)
    {
        return Err(LocalApiError::bad_request(
            "Plugin credential component is not present in the active signed inventory",
        ));
    }
    PluginCredentialScope::new(
        owner_user_id,
        device_id,
        plugin_id,
        installation.version.release_id,
        component_key,
        secret_name,
    )
    .map_err(|error| LocalApiError::bad_request(error.to_string()))
}

async fn complete_plugin_oauth(
    runtime: LocalRuntime,
    request: CompletePluginOAuthRequest,
) -> Result<Json<LocalPluginOAuthConnection>, LocalApiError> {
    if let Some(error) = request.error.as_deref() {
        if request.code.is_some() {
            return Err(LocalApiError::bad_request(
                "Plugin OAuth callback cannot contain both code and error",
            ));
        }
        let failure = runtime
            .plugin_oauth
            .consume_authorization_error(
                request.state.as_str(),
                error,
                request.error_description.as_deref(),
            )
            .map_err(plugin_oauth_callback_error)?;
        return Err(LocalApiError::conflict_code(failure.code, failure.message));
    }
    if request.error_description.is_some() {
        return Err(LocalApiError::bad_request(
            "Plugin OAuth callback error_description requires error",
        ));
    }
    let code = request
        .code
        .as_deref()
        .filter(|code| !code.trim().is_empty())
        .ok_or_else(|| LocalApiError::bad_request("Plugin OAuth callback requires code"))?;
    let connection = runtime
        .plugin_oauth
        .complete_authorization(request.state.as_str(), code)
        .await
        .map_err(plugin_oauth_callback_error)?;
    Ok(Json(connection))
}

fn plugin_oauth_callback_error(error: anyhow::Error) -> LocalApiError {
    let message = error.to_string();
    let code = if message.contains("Plugin OAuth state is invalid or expired") {
        "plugin_oauth_state_invalid"
    } else {
        "plugin_oauth_callback_failed"
    };
    LocalApiError::conflict_code(code, message)
}

async fn active_plugin_installation(
    runtime: &LocalRuntime,
    plugin_id: &str,
) -> Result<ActivePluginInstallation, LocalApiError> {
    let installer = runtime.plugin_installer.clone();
    let plugin_id = plugin_id.to_string();
    tokio::task::spawn_blocking(move || installer.active_installation(plugin_id.as_str()))
        .await
        .map_err(|error| anyhow::anyhow!("join active Plugin validation task failed: {error}"))??
        .ok_or_else(|| LocalApiError::conflict("Plugin is not installed and active"))
}
