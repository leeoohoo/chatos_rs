// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use axum::extract::Query;
use axum::response::Html;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::StreamExt;
use mongodb::bson::DateTime as BsonDateTime;
use reqwest::redirect::Policy;
use reqwest::{StatusCode as HttpStatusCode, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use chatos_plugin_management_sdk::PluginMcpServer;

use super::plugin_cloud_credentials::{
    normalize_scopes, oauth_aad, permissions_for_release, require_oauth_permissions,
    require_visible_cloud_mcp_release, validate_identifier,
};
use super::*;

#[path = "plugin_cloud_oauth/support.rs"]
mod support;
use support::*;

const OAUTH_STATE_BYTES: usize = 32;
const OAUTH_CODE_VERIFIER_BYTES: usize = 48;
const MAX_OAUTH_TEXT_BYTES: usize = 8 * 1024;
const REFRESH_LEASE_SECONDS: i64 = 30;
const REFRESH_WAIT_ATTEMPTS: usize = 20;

#[derive(Deserialize)]
pub(super) struct PluginCloudOAuthCallbackQuery {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProtectedResourceMetadata {
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    authorization_servers: Vec<String>,
    #[serde(default)]
    scopes_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    registration_endpoint: Option<String>,
    #[serde(default)]
    response_types_supported: Vec<String>,
    #[serde(default)]
    grant_types_supported: Vec<String>,
    #[serde(default)]
    code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    token_endpoint_auth_methods_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DynamicClientRegistrationResponse {
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
}

#[derive(Serialize)]
struct DynamicClientRegistrationRequest<'a> {
    client_name: &'a str,
    redirect_uris: [&'a str; 1],
    grant_types: [&'a str; 2],
    response_types: [&'a str; 1],
    token_endpoint_auth_method: &'a str,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthErrorResponse {
    #[serde(default)]
    error: Option<String>,
}

struct DiscoveredOAuthServer {
    authorization_server: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
    token_endpoint_auth_methods_supported: Vec<String>,
}

struct OAuthClientRegistration {
    client_id: String,
    client_secret: Option<Zeroizing<String>>,
    token_endpoint_auth_method: String,
}

#[derive(Debug)]
enum OAuthTokenRequestError {
    ReauthorizationRequired,
    Transient(String),
}

pub(super) async fn begin_plugin_cloud_oauth_authorization(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((plugin_id, release_id, component_key)): Path<(String, String, String)>,
    Json(mut request): Json<BeginPluginCloudOAuthAuthorizationRequest>,
) -> Result<Response, ApiError> {
    let provider = validate_identifier(request.provider.as_str(), "provider", 96)?;
    let scopes = normalize_scopes(std::mem::take(&mut request.scopes))?;
    let (release, bundle) = require_visible_cloud_mcp_release(
        &state,
        &user,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    let PluginMcpServer::Http {
        oauth_resource: Some(resource),
        headers,
        ..
    } = bundle.effective_runtime()
    else {
        return Err(ApiError::bad_request(
            "Plugin cloud OAuth requires an HTTP MCP runtime with oauth_resource",
        ));
    };
    if headers
        .keys()
        .any(|name| name.trim().eq_ignore_ascii_case("authorization"))
    {
        return Err(ApiError::conflict(
            "Plugin cloud OAuth cannot override an immutable Authorization header",
        ));
    }
    let permissions = permissions_for_release(&release, component_key.as_str());
    require_oauth_permissions(provider.as_str(), scopes.as_slice(), permissions.as_slice())?;
    let discovered = discover_oauth_server(
        &state,
        resource.as_str(),
        request.authorization_server.as_deref(),
        scopes.as_slice(),
    )
    .await?;
    let redirect_uri = state.config.oauth_callback_url();
    let client = resolve_oauth_client(
        &state,
        &discovered,
        redirect_uri.as_str(),
        request.client_id.take(),
        request.client_secret.take(),
        request.token_endpoint_auth_method.take(),
    )
    .await?;

    let flow_id = Uuid::new_v4().to_string();
    let state_secret = random_secret(OAUTH_STATE_BYTES);
    let state_sha256 = sha256_hex(state_secret.as_bytes());
    let code_verifier = Zeroizing::new(random_secret(OAUTH_CODE_VERIFIER_BYTES));
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    let authorization_aad = oauth_authorization_aad(flow_id.as_str(), state_sha256.as_str());
    let encrypted_code_verifier = state
        .cloud_secret_cipher
        .encrypt(code_verifier.as_str(), authorization_aad.as_str())
        .map_err(ApiError::internal)?;
    let encrypted_client_secret = client
        .client_secret
        .as_deref()
        .map(|secret| {
            state
                .cloud_secret_cipher
                .encrypt(secret, authorization_aad.as_str())
        })
        .transpose()
        .map_err(ApiError::internal)?;
    let expires_at = Utc::now()
        + ChronoDuration::from_std(state.config.oauth_flow_ttl)
            .map_err(|error| ApiError::internal(error.to_string()))?;
    let authorization_url = build_authorization_url(
        discovered.authorization_endpoint.as_str(),
        client.client_id.as_str(),
        redirect_uri.as_str(),
        state_secret.as_str(),
        code_challenge.as_str(),
        resource.as_str(),
        scopes.as_slice(),
    )?;
    let owner_user_id = user.effective_owner_user_id();
    state
        .store
        .insert_plugin_cloud_oauth_authorization(&StoredPluginCloudOAuthAuthorizationSession {
            id: flow_id.clone(),
            state_sha256,
            owner_user_id: owner_user_id.to_string(),
            plugin_id: plugin_id.clone(),
            release_id: release_id.clone(),
            component_key: component_key.clone(),
            provider,
            resource: resource.clone(),
            scopes,
            authorization_server: discovered.authorization_server,
            authorization_endpoint: discovered.authorization_endpoint,
            token_endpoint: discovered.token_endpoint,
            client_id: client.client_id,
            token_endpoint_auth_method: client.token_endpoint_auth_method,
            encrypted_client_secret,
            encrypted_code_verifier,
            redirect_uri,
            created_at: now_rfc3339(),
            expires_at: BsonDateTime::from_millis(expires_at.timestamp_millis()),
        })
        .await
        .map_err(ApiError::internal)?;
    write_oauth_audit(
        &state,
        PLUGIN_AUDIT_BEGIN_CLOUD_OAUTH,
        owner_user_id,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
        "success",
    )
    .await?;
    let mut response = Json(BeginPluginCloudOAuthAuthorizationResponse {
        flow_id,
        authorization_url: authorization_url.to_string(),
        callback_origin: callback_origin(state.config.oauth_public_base_url.as_str())?,
        expires_at: expires_at.to_rfc3339(),
    })
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    Ok(response)
}

pub(super) async fn complete_plugin_cloud_oauth_authorization(
    State(state): State<AppState>,
    Query(mut query): Query<PluginCloudOAuthCallbackQuery>,
) -> Response {
    let result = complete_plugin_cloud_oauth_authorization_inner(&state, &mut query).await;
    oauth_callback_response(state.config.oauth_frontend_origin.as_str(), result)
}

async fn complete_plugin_cloud_oauth_authorization_inner(
    state: &AppState,
    query: &mut PluginCloudOAuthCallbackQuery,
) -> Result<PluginCloudOAuthConnectionRecord, String> {
    let state_secret = query
        .state
        .take()
        .filter(|value| is_bounded_oauth_text(value, 512))
        .ok_or_else(|| "OAuth callback state is missing or invalid".to_string())?;
    let authorization = state
        .store
        .consume_plugin_cloud_oauth_authorization(sha256_hex(state_secret.as_bytes()).as_str())
        .await
        .map_err(|_| "OAuth authorization session could not be consumed".to_string())?
        .ok_or_else(|| {
            "OAuth authorization session is missing, expired, or already used".to_string()
        })?;
    if authorization.expires_at.timestamp_millis() <= Utc::now().timestamp_millis() {
        return Err("OAuth authorization session expired".to_string());
    }
    if query.error.take().is_some() {
        let _ = query.error_description.take();
        write_oauth_audit(
            state,
            PLUGIN_AUDIT_COMPLETE_CLOUD_OAUTH,
            authorization.owner_user_id.as_str(),
            authorization.plugin_id.as_str(),
            authorization.release_id.as_str(),
            authorization.component_key.as_str(),
            "denied",
        )
        .await
        .map_err(|_| "OAuth denial could not be audited".to_string())?;
        return Err("OAuth authorization was denied by the provider".to_string());
    }
    let code = Zeroizing::new(
        query
            .code
            .take()
            .filter(|value| is_bounded_oauth_text(value, MAX_OAUTH_TEXT_BYTES))
            .ok_or_else(|| "OAuth callback code is missing or invalid".to_string())?,
    );
    let authorization_aad = oauth_authorization_aad(
        authorization.id.as_str(),
        authorization.state_sha256.as_str(),
    );
    let code_verifier = state
        .cloud_secret_cipher
        .decrypt(
            authorization.encrypted_code_verifier.as_str(),
            authorization_aad.as_str(),
        )
        .map_err(|_| "OAuth PKCE verifier could not be decrypted".to_string())?;
    let client_secret = authorization
        .encrypted_client_secret
        .as_deref()
        .map(|encrypted| {
            state
                .cloud_secret_cipher
                .decrypt(encrypted, authorization_aad.as_str())
        })
        .transpose()
        .map_err(|_| "OAuth client secret could not be decrypted".to_string())?;
    let fields = vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code.as_str().to_string()),
        (
            "redirect_uri".to_string(),
            authorization.redirect_uri.clone(),
        ),
        (
            "code_verifier".to_string(),
            code_verifier.as_str().to_string(),
        ),
        ("resource".to_string(), authorization.resource.clone()),
    ];
    let token = request_oauth_token(
        state,
        authorization.token_endpoint.as_str(),
        authorization.client_id.as_str(),
        client_secret.as_deref().map(|value| value.as_str()),
        authorization.token_endpoint_auth_method.as_str(),
        fields,
    )
    .await
    .map_err(|error| match error {
        OAuthTokenRequestError::ReauthorizationRequired => {
            "OAuth authorization code was rejected".to_string()
        }
        OAuthTokenRequestError::Transient(_) => {
            "OAuth token endpoint is temporarily unavailable".to_string()
        }
    })?;
    let connection = persist_authorized_connection(
        state,
        &authorization,
        token,
        client_secret.as_deref().map(|value| value.as_str()),
    )
    .await
    .map_err(|_| "OAuth connection could not be stored".to_string())?;
    write_oauth_audit(
        state,
        PLUGIN_AUDIT_COMPLETE_CLOUD_OAUTH,
        authorization.owner_user_id.as_str(),
        authorization.plugin_id.as_str(),
        authorization.release_id.as_str(),
        authorization.component_key.as_str(),
        "success",
    )
    .await
    .map_err(|_| "OAuth completion could not be audited".to_string())?;
    Ok(connection)
}

pub(super) async fn refresh_cloud_oauth_connection_if_needed(
    state: &AppState,
    record: StoredPluginCloudOAuthConnection,
    minimum_valid_until_unix: Option<i64>,
) -> Result<StoredPluginCloudOAuthConnection, ApiError> {
    let required_valid_until_unix = minimum_valid_until_unix
        .unwrap_or_else(|| Utc::now().timestamp())
        .max(Utc::now().timestamp())
        .saturating_add(state.config.oauth_refresh_skew.as_secs() as i64);
    if !oauth_access_token_needs_refresh(&record.connection, required_valid_until_unix)? {
        return Ok(record);
    }
    if !record.connection.refreshable {
        return Err(ApiError::conflict(
            "Plugin OAuth access token is expired or too close to expiry and requires browser reauthorization",
        ));
    }
    let lease_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let claimed = state
        .store
        .claim_plugin_cloud_oauth_refresh(
            record.connection.id.as_str(),
            record.connection.revision.as_str(),
            lease_id.as_str(),
            BsonDateTime::from_millis(now.timestamp_millis()),
            BsonDateTime::from_millis(
                (now + ChronoDuration::seconds(REFRESH_LEASE_SECONDS)).timestamp_millis(),
            ),
        )
        .await
        .map_err(ApiError::internal)?;
    if !claimed {
        for _ in 0..REFRESH_WAIT_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let latest = state
                .store
                .get_plugin_cloud_oauth_connection_by_id(record.connection.id.as_str())
                .await
                .map_err(ApiError::internal)?
                .ok_or_else(|| ApiError::conflict("Plugin OAuth connection disappeared"))?;
            if latest.connection.revision != record.connection.revision {
                if !latest.connection.connected || latest.connection.needs_auth {
                    return Err(ApiError::conflict(
                        "Plugin OAuth token refresh requires browser reauthorization",
                    ));
                }
                if oauth_access_token_needs_refresh(&latest.connection, required_valid_until_unix)?
                {
                    return Err(ApiError::conflict(
                        "Plugin OAuth token refresh completed without a usable access token",
                    ));
                }
                return Ok(latest);
            }
        }
        return Err(ApiError::conflict(
            "Plugin OAuth token refresh is already in progress",
        ));
    }

    let refresh_result =
        refresh_claimed_cloud_oauth_connection(state, &record, lease_id.as_str()).await;
    if refresh_result.is_err() {
        let _ = state
            .store
            .release_plugin_cloud_oauth_refresh(record.connection.id.as_str(), lease_id.as_str())
            .await;
    }
    let refreshed = refresh_result?;
    if oauth_access_token_needs_refresh(&refreshed.connection, required_valid_until_unix)? {
        return Err(ApiError::conflict(
            "Plugin OAuth refreshed access token does not cover the requested Runtime Session lifetime",
        ));
    }
    Ok(refreshed)
}

async fn refresh_claimed_cloud_oauth_connection(
    state: &AppState,
    record: &StoredPluginCloudOAuthConnection,
    lease_id: &str,
) -> Result<StoredPluginCloudOAuthConnection, ApiError> {
    let client = record
        .oauth_client
        .as_ref()
        .ok_or_else(|| ApiError::conflict("Plugin OAuth refresh client is unavailable"))?;
    let encrypted_refresh_token = record
        .encrypted_refresh_token
        .as_deref()
        .ok_or_else(|| ApiError::conflict("Plugin OAuth refresh token is unavailable"))?;
    let old_aad = oauth_aad(&record.connection);
    let refresh_token = state
        .cloud_secret_cipher
        .decrypt(encrypted_refresh_token, old_aad.as_str())
        .map_err(ApiError::internal)?;
    let client_secret = client
        .encrypted_client_secret
        .as_deref()
        .map(|encrypted| {
            state
                .cloud_secret_cipher
                .decrypt(encrypted, old_aad.as_str())
        })
        .transpose()
        .map_err(ApiError::internal)?;
    let fields = vec![
        ("grant_type".to_string(), "refresh_token".to_string()),
        (
            "refresh_token".to_string(),
            refresh_token.as_str().to_string(),
        ),
        ("resource".to_string(), record.connection.resource.clone()),
        ("scope".to_string(), record.connection.scopes.join(" ")),
    ];
    let token = match request_oauth_token(
        state,
        client.token_endpoint.as_str(),
        client.client_id.as_str(),
        client_secret.as_deref().map(|value| value.as_str()),
        client.token_endpoint_auth_method.as_str(),
        fields,
    )
    .await
    {
        Ok(token) => token,
        Err(OAuthTokenRequestError::ReauthorizationRequired) => {
            mark_cloud_oauth_needs_reauthorization(state, record, lease_id).await?;
            return Err(ApiError::conflict(
                "Plugin OAuth refresh was rejected and requires browser reauthorization",
            ));
        }
        Err(OAuthTokenRequestError::Transient(message)) => {
            return Err(ApiError::bad_gateway(message));
        }
    };
    let scopes = normalized_token_scopes(token.scope.as_deref(), &record.connection.scopes)?;
    if token.expires_in.is_none() {
        return Err(ApiError::conflict(
            "OAuth refresh response must include expires_in",
        ));
    }
    let access_token = validate_token_secret(token.access_token, "OAuth access token")?;
    let rotated_refresh_token = token
        .refresh_token
        .map(|value| validate_token_secret(value, "OAuth refresh token"))
        .transpose()?
        .unwrap_or(refresh_token);
    validate_bearer_token_type(token.token_type.as_deref())?;
    let mut connection = record.connection.clone();
    connection.scopes = scopes;
    connection.connected = true;
    connection.needs_auth = false;
    connection.refreshable = true;
    connection.expires_at = oauth_expiry(token.expires_in)?;
    connection.revision = Uuid::new_v4().to_string();
    connection.updated_at = now_rfc3339();
    let new_aad = oauth_aad(&connection);
    let encrypted_access_token = state
        .cloud_secret_cipher
        .encrypt(access_token.as_str(), new_aad.as_str())
        .map_err(ApiError::internal)?;
    let encrypted_refresh_token = state
        .cloud_secret_cipher
        .encrypt(rotated_refresh_token.as_str(), new_aad.as_str())
        .map_err(ApiError::internal)?;
    let encrypted_client_secret = client_secret
        .as_deref()
        .map(|secret| {
            state
                .cloud_secret_cipher
                .encrypt(secret.as_str(), new_aad.as_str())
        })
        .transpose()
        .map_err(ApiError::internal)?;
    let refreshed = StoredPluginCloudOAuthConnection {
        connection: connection.clone(),
        encrypted_access_token: Some(encrypted_access_token),
        encrypted_refresh_token: Some(encrypted_refresh_token),
        oauth_client: Some(StoredPluginCloudOAuthClient {
            authorization_server: client.authorization_server.clone(),
            token_endpoint: client.token_endpoint.clone(),
            client_id: client.client_id.clone(),
            token_endpoint_auth_method: client.token_endpoint_auth_method.clone(),
            encrypted_client_secret,
        }),
        refresh_lease_id: None,
        refresh_lease_expires_at: None,
    };
    let replaced = state
        .store
        .replace_claimed_plugin_cloud_oauth_connection(&refreshed, lease_id)
        .await
        .map_err(ApiError::internal)?;
    if !replaced {
        return Err(ApiError::conflict(
            "Plugin OAuth refresh lease was lost before token rotation completed",
        ));
    }
    write_oauth_audit(
        state,
        PLUGIN_AUDIT_REFRESH_CLOUD_OAUTH,
        connection.owner_user_id.as_str(),
        connection.plugin_id.as_str(),
        connection.release_id.as_str(),
        connection.component_key.as_str(),
        "success",
    )
    .await?;
    Ok(refreshed)
}

async fn mark_cloud_oauth_needs_reauthorization(
    state: &AppState,
    record: &StoredPluginCloudOAuthConnection,
    lease_id: &str,
) -> Result<(), ApiError> {
    let mut connection = record.connection.clone();
    connection.connected = false;
    connection.needs_auth = true;
    connection.refreshable = false;
    connection.expires_at = None;
    connection.revision = Uuid::new_v4().to_string();
    connection.updated_at = now_rfc3339();
    let replacement = StoredPluginCloudOAuthConnection {
        connection: connection.clone(),
        encrypted_access_token: None,
        encrypted_refresh_token: None,
        oauth_client: None,
        refresh_lease_id: None,
        refresh_lease_expires_at: None,
    };
    let replaced = state
        .store
        .replace_claimed_plugin_cloud_oauth_connection(&replacement, lease_id)
        .await
        .map_err(ApiError::internal)?;
    if !replaced {
        return Err(ApiError::conflict(
            "Plugin OAuth refresh lease was lost while requiring reauthorization",
        ));
    }
    write_oauth_audit(
        state,
        PLUGIN_AUDIT_REAUTHORIZE_CLOUD_OAUTH,
        connection.owner_user_id.as_str(),
        connection.plugin_id.as_str(),
        connection.release_id.as_str(),
        connection.component_key.as_str(),
        "required",
    )
    .await
}

async fn persist_authorized_connection(
    state: &AppState,
    authorization: &StoredPluginCloudOAuthAuthorizationSession,
    token: OAuthTokenResponse,
    client_secret: Option<&str>,
) -> Result<PluginCloudOAuthConnectionRecord, ApiError> {
    validate_bearer_token_type(token.token_type.as_deref())?;
    let scopes = normalized_token_scopes(token.scope.as_deref(), &authorization.scopes)?;
    let access_token = validate_token_secret(token.access_token, "OAuth access token")?;
    let refresh_token = token
        .refresh_token
        .map(|value| validate_token_secret(value, "OAuth refresh token"))
        .transpose()?;
    if refresh_token.is_some() && token.expires_in.is_none() {
        return Err(ApiError::conflict(
            "OAuth token response with refresh_token must include expires_in",
        ));
    }
    let existing = state
        .store
        .get_plugin_cloud_oauth_connection(
            authorization.owner_user_id.as_str(),
            authorization.plugin_id.as_str(),
            authorization.release_id.as_str(),
            authorization.component_key.as_str(),
            authorization.provider.as_str(),
            authorization.resource.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    let connection = PluginCloudOAuthConnectionRecord {
        id: existing
            .as_ref()
            .map(|record| record.connection.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        owner_user_id: authorization.owner_user_id.clone(),
        plugin_id: authorization.plugin_id.clone(),
        release_id: authorization.release_id.clone(),
        component_key: authorization.component_key.clone(),
        provider: authorization.provider.clone(),
        resource: authorization.resource.clone(),
        scopes,
        connected: true,
        needs_auth: false,
        refreshable: refresh_token.is_some(),
        expires_at: oauth_expiry(token.expires_in)?,
        account_display: None,
        revision: Uuid::new_v4().to_string(),
        updated_at: now_rfc3339(),
    };
    let aad = oauth_aad(&connection);
    let encrypted_access_token = state
        .cloud_secret_cipher
        .encrypt(access_token.as_str(), aad.as_str())
        .map_err(ApiError::internal)?;
    let encrypted_refresh_token = refresh_token
        .as_deref()
        .map(|secret| state.cloud_secret_cipher.encrypt(secret, aad.as_str()))
        .transpose()
        .map_err(ApiError::internal)?;
    let encrypted_client_secret = client_secret
        .map(|secret| state.cloud_secret_cipher.encrypt(secret, aad.as_str()))
        .transpose()
        .map_err(ApiError::internal)?;
    state
        .store
        .replace_plugin_cloud_oauth_connection(&StoredPluginCloudOAuthConnection {
            connection: connection.clone(),
            encrypted_access_token: Some(encrypted_access_token),
            encrypted_refresh_token,
            oauth_client: Some(StoredPluginCloudOAuthClient {
                authorization_server: authorization.authorization_server.clone(),
                token_endpoint: authorization.token_endpoint.clone(),
                client_id: authorization.client_id.clone(),
                token_endpoint_auth_method: authorization.token_endpoint_auth_method.clone(),
                encrypted_client_secret,
            }),
            refresh_lease_id: None,
            refresh_lease_expires_at: None,
        })
        .await
        .map_err(ApiError::internal)?;
    Ok(connection)
}

#[cfg(test)]
#[path = "plugin_cloud_oauth/tests.rs"]
mod tests;
