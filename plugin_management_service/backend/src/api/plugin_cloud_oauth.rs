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
    } = &bundle.runtime
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

async fn discover_oauth_server(
    state: &AppState,
    resource: &str,
    requested_authorization_server: Option<&str>,
    scopes: &[String],
) -> Result<DiscoveredOAuthServer, ApiError> {
    let resource_url = validate_public_https_url(resource, "Plugin OAuth resource")?;
    let mut resource_metadata = None;
    for url in protected_resource_metadata_urls(&resource_url)? {
        if let Some(metadata) =
            fetch_json_optional::<ProtectedResourceMetadata>(state, &url).await?
        {
            resource_metadata = Some(metadata);
            break;
        }
    }
    let resource_metadata = resource_metadata.ok_or_else(|| {
        ApiError::bad_gateway("Plugin OAuth protected resource metadata was not found")
    })?;
    if let Some(metadata_resource) = resource_metadata.resource.as_deref() {
        let metadata_resource = validate_public_https_url(
            metadata_resource,
            "Plugin OAuth protected resource metadata resource",
        )?;
        if normalized_url(&metadata_resource) != normalized_url(&resource_url) {
            return Err(ApiError::conflict(
                "Plugin OAuth protected resource metadata does not match the immutable resource",
            ));
        }
    }
    if !resource_metadata.scopes_supported.is_empty() {
        let supported = normalize_scopes(resource_metadata.scopes_supported)?;
        if scopes.iter().any(|scope| !supported.contains(scope)) {
            return Err(ApiError::conflict(
                "Plugin OAuth requested scopes are not supported by the protected resource",
            ));
        }
    }
    let authorization_servers = resource_metadata
        .authorization_servers
        .into_iter()
        .map(|value| validate_public_https_url(value.as_str(), "OAuth authorization server"))
        .collect::<Result<Vec<_>, _>>()?;
    if authorization_servers.is_empty() {
        return Err(ApiError::bad_gateway(
            "Plugin OAuth protected resource did not publish an authorization server",
        ));
    }
    let authorization_server = match requested_authorization_server {
        Some(value) => {
            let requested = validate_public_https_url(value, "authorization_server")?;
            authorization_servers
                .into_iter()
                .find(|candidate| normalized_url(candidate) == normalized_url(&requested))
                .ok_or_else(|| {
                    ApiError::conflict(
                        "Requested OAuth authorization server is not authorized by the protected resource",
                    )
                })?
        }
        None if authorization_servers.len() == 1 => authorization_servers
            .into_iter()
            .next()
            .expect("single authorization server"),
        None => {
            return Err(ApiError::bad_request(
                "Plugin OAuth resource publishes multiple authorization servers; select one explicitly",
            ));
        }
    };
    let mut server_metadata = None;
    for url in authorization_server_metadata_urls(&authorization_server)? {
        if let Some(metadata) =
            fetch_json_optional::<AuthorizationServerMetadata>(state, &url).await?
        {
            server_metadata = Some(metadata);
            break;
        }
    }
    let metadata = server_metadata.ok_or_else(|| {
        ApiError::bad_gateway("OAuth authorization server metadata was not found")
    })?;
    let issuer = validate_public_https_url(metadata.issuer.as_str(), "OAuth issuer")?;
    if normalized_url(&issuer) != normalized_url(&authorization_server) {
        return Err(ApiError::conflict(
            "OAuth authorization server metadata issuer does not match the protected resource",
        ));
    }
    if !metadata.response_types_supported.is_empty()
        && !metadata
            .response_types_supported
            .iter()
            .any(|value| value == "code")
    {
        return Err(ApiError::conflict(
            "OAuth authorization server does not support authorization code flow",
        ));
    }
    if !metadata.grant_types_supported.is_empty()
        && !metadata
            .grant_types_supported
            .iter()
            .any(|value| value == "authorization_code")
    {
        return Err(ApiError::conflict(
            "OAuth authorization server does not support authorization code grant",
        ));
    }
    if !metadata.code_challenge_methods_supported.is_empty()
        && !metadata
            .code_challenge_methods_supported
            .iter()
            .any(|value| value == "S256")
    {
        return Err(ApiError::conflict(
            "OAuth authorization server does not support PKCE S256",
        ));
    }
    let authorization_endpoint = validate_public_https_url(
        metadata.authorization_endpoint.as_str(),
        "OAuth authorization endpoint",
    )?;
    ensure_public_url_host(&authorization_endpoint).await?;
    let token_endpoint =
        validate_public_https_url(metadata.token_endpoint.as_str(), "OAuth token endpoint")?;
    ensure_public_url_host(&token_endpoint).await?;
    let registration_endpoint = metadata
        .registration_endpoint
        .as_deref()
        .map(|value| validate_public_https_url(value, "OAuth registration endpoint"))
        .transpose()?;
    if let Some(endpoint) = registration_endpoint.as_ref() {
        ensure_public_url_host(endpoint).await?;
    }
    Ok(DiscoveredOAuthServer {
        authorization_server: authorization_server.to_string(),
        authorization_endpoint: authorization_endpoint.to_string(),
        token_endpoint: token_endpoint.to_string(),
        registration_endpoint: registration_endpoint.map(|url| url.to_string()),
        token_endpoint_auth_methods_supported: metadata.token_endpoint_auth_methods_supported,
    })
}

async fn resolve_oauth_client(
    state: &AppState,
    server: &DiscoveredOAuthServer,
    redirect_uri: &str,
    client_id: Option<String>,
    client_secret: Option<String>,
    requested_auth_method: Option<String>,
) -> Result<OAuthClientRegistration, ApiError> {
    if let Some(client_id) = client_id {
        let client_id = validate_oauth_text(client_id.as_str(), "client_id", 1_024)?;
        let client_secret = client_secret
            .map(|value| validate_token_secret(value, "OAuth client secret"))
            .transpose()?;
        let method = normalize_token_endpoint_auth_method(
            requested_auth_method.as_deref(),
            client_secret.is_some(),
        )?;
        require_supported_auth_method(
            method.as_str(),
            server.token_endpoint_auth_methods_supported.as_slice(),
        )?;
        return Ok(OAuthClientRegistration {
            client_id,
            client_secret,
            token_endpoint_auth_method: method,
        });
    }
    if client_secret.is_some() || requested_auth_method.is_some() {
        return Err(ApiError::bad_request(
            "OAuth client_secret and token_endpoint_auth_method require client_id",
        ));
    }
    let registration_endpoint = server.registration_endpoint.as_deref().ok_or_else(|| {
        ApiError::conflict(
            "OAuth server does not support dynamic client registration; configure client_id",
        )
    })?;
    require_supported_auth_method(
        "none",
        server.token_endpoint_auth_methods_supported.as_slice(),
    )?;
    let response: DynamicClientRegistrationResponse = post_json(
        state,
        &validate_public_https_url(registration_endpoint, "OAuth registration endpoint")?,
        &DynamicClientRegistrationRequest {
            client_name: "ChatOS Plugin MCP",
            redirect_uris: [redirect_uri],
            grant_types: ["authorization_code", "refresh_token"],
            response_types: ["code"],
            token_endpoint_auth_method: "none",
        },
    )
    .await?;
    let client_id = validate_oauth_text(response.client_id.as_str(), "client_id", 1_024)?;
    let client_secret = response
        .client_secret
        .map(|value| validate_token_secret(value, "OAuth client secret"))
        .transpose()?;
    let method = normalize_token_endpoint_auth_method(
        response.token_endpoint_auth_method.as_deref(),
        client_secret.is_some(),
    )?;
    require_supported_auth_method(
        method.as_str(),
        server.token_endpoint_auth_methods_supported.as_slice(),
    )?;
    Ok(OAuthClientRegistration {
        client_id,
        client_secret,
        token_endpoint_auth_method: method,
    })
}

fn build_authorization_url(
    endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
    resource: &str,
    scopes: &[String],
) -> Result<Url, ApiError> {
    let mut url = validate_public_https_url(endpoint, "OAuth authorization endpoint")?;
    {
        let mut query = url.query_pairs_mut();
        query.clear();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("state", state);
        query.append_pair("code_challenge", code_challenge);
        query.append_pair("code_challenge_method", "S256");
        query.append_pair("resource", resource);
        query.append_pair("scope", scopes.join(" ").as_str());
    }
    Ok(url)
}

async fn request_oauth_token(
    state: &AppState,
    endpoint: &str,
    client_id: &str,
    client_secret: Option<&str>,
    auth_method: &str,
    mut fields: Vec<(String, String)>,
) -> Result<OAuthTokenResponse, OAuthTokenRequestError> {
    let url = validate_public_https_url(endpoint, "OAuth token endpoint")
        .map_err(|error| OAuthTokenRequestError::Transient(error.message))?;
    let client = public_http_client(state, &url)
        .await
        .map_err(|error| OAuthTokenRequestError::Transient(error.message))?;
    let mut request = client.post(url);
    match auth_method {
        "none" => fields.push(("client_id".to_string(), client_id.to_string())),
        "client_secret_post" => {
            let secret = client_secret.ok_or(OAuthTokenRequestError::ReauthorizationRequired)?;
            fields.push(("client_id".to_string(), client_id.to_string()));
            fields.push(("client_secret".to_string(), secret.to_string()));
        }
        "client_secret_basic" => {
            let secret = client_secret.ok_or(OAuthTokenRequestError::ReauthorizationRequired)?;
            request = request.basic_auth(client_id, Some(secret));
        }
        _ => return Err(OAuthTokenRequestError::ReauthorizationRequired),
    }
    request = request.form(&fields);
    for (_, value) in &mut fields {
        value.zeroize();
    }
    let response = request
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| {
            OAuthTokenRequestError::Transient("OAuth token endpoint request failed".to_string())
        })?;
    let status = response.status();
    let body = Zeroizing::new(
        bounded_response_body(response, state.config.oauth_max_response_bytes)
            .await
            .map_err(|error| OAuthTokenRequestError::Transient(error.message))?,
    );
    if !status.is_success() {
        let error = serde_json::from_slice::<OAuthErrorResponse>(body.as_slice())
            .ok()
            .and_then(|payload| payload.error)
            .unwrap_or_default();
        if status == HttpStatusCode::BAD_REQUEST
            || status == HttpStatusCode::UNAUTHORIZED
            || matches!(
                error.as_str(),
                "invalid_grant" | "invalid_client" | "unauthorized_client"
            )
        {
            return Err(OAuthTokenRequestError::ReauthorizationRequired);
        }
        return Err(OAuthTokenRequestError::Transient(format!(
            "OAuth token endpoint returned {status}"
        )));
    }
    serde_json::from_slice(body.as_slice()).map_err(|_| {
        OAuthTokenRequestError::Transient("OAuth token response is invalid".to_string())
    })
}

async fn fetch_json_optional<T: DeserializeOwned>(
    state: &AppState,
    url: &Url,
) -> Result<Option<T>, ApiError> {
    let client = public_http_client(state, url).await?;
    let response = client
        .get(url.clone())
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("OAuth metadata request failed"))?;
    if response.status() == HttpStatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(ApiError::bad_gateway(format!(
            "OAuth metadata endpoint returned {}",
            response.status()
        )));
    }
    let body = Zeroizing::new(
        bounded_response_body(response, state.config.oauth_max_response_bytes).await?,
    );
    serde_json::from_slice(body.as_slice())
        .map(Some)
        .map_err(|_| ApiError::bad_gateway("OAuth metadata response is invalid"))
}

async fn post_json<T: DeserializeOwned, B: Serialize + ?Sized>(
    state: &AppState,
    url: &Url,
    body: &B,
) -> Result<T, ApiError> {
    let client = public_http_client(state, url).await?;
    let response = client
        .post(url.clone())
        .header(header::ACCEPT, "application/json")
        .json(body)
        .send()
        .await
        .map_err(|_| ApiError::bad_gateway("OAuth client registration request failed"))?;
    if !response.status().is_success() {
        return Err(ApiError::bad_gateway(format!(
            "OAuth client registration returned {}",
            response.status()
        )));
    }
    let body = Zeroizing::new(
        bounded_response_body(response, state.config.oauth_max_response_bytes).await?,
    );
    serde_json::from_slice(body.as_slice())
        .map_err(|_| ApiError::bad_gateway("OAuth client registration response is invalid"))
}

async fn public_http_client(state: &AppState, url: &Url) -> Result<reqwest::Client, ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::bad_request("OAuth URL host is missing"))?;
    let addresses = resolve_public_url_addresses(url).await?;
    reqwest::Client::builder()
        .timeout(state.config.oauth_request_timeout)
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .resolve_to_addrs(host, addresses.as_slice())
        .build()
        .map_err(|error| ApiError::internal(format!("build OAuth HTTP client failed: {error}")))
}

async fn ensure_public_url_host(url: &Url) -> Result<(), ApiError> {
    resolve_public_url_addresses(url).await.map(|_| ())
}

async fn resolve_public_url_addresses(url: &Url) -> Result<Vec<SocketAddr>, ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::bad_request("OAuth URL host is missing"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ApiError::bad_request("OAuth URL port is invalid"))?;
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ApiError::bad_gateway("OAuth host DNS resolution failed"))?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(ApiError::bad_request(
            "OAuth endpoints must resolve only to public addresses",
        ));
    }
    Ok(addresses)
}

async fn bounded_response_body(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, ApiError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(ApiError::bad_gateway(
            "OAuth response exceeds the size limit",
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::bad_gateway("OAuth response read failed"))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(ApiError::bad_gateway(
                "OAuth response exceeds the size limit",
            ));
        }
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body)
}

fn protected_resource_metadata_urls(resource: &Url) -> Result<Vec<Url>, ApiError> {
    let mut root = resource.clone();
    root.set_query(None);
    root.set_fragment(None);
    root.set_path("/.well-known/oauth-protected-resource");
    let resource_path = resource.path().trim_matches('/');
    let mut urls = Vec::new();
    if !resource_path.is_empty() {
        let mut path_specific = root.clone();
        path_specific
            .set_path(format!("/.well-known/oauth-protected-resource/{resource_path}").as_str());
        urls.push(path_specific);
    }
    urls.push(root);
    Ok(urls)
}

fn authorization_server_metadata_urls(issuer: &Url) -> Result<Vec<Url>, ApiError> {
    let mut base = issuer.clone();
    base.set_query(None);
    base.set_fragment(None);
    let issuer_path = issuer.path().trim_matches('/');
    let mut oauth = base.clone();
    oauth.set_path(
        if issuer_path.is_empty() {
            "/.well-known/oauth-authorization-server".to_string()
        } else {
            format!("/.well-known/oauth-authorization-server/{issuer_path}")
        }
        .as_str(),
    );
    let mut oidc = base;
    oidc.set_path(
        if issuer_path.is_empty() {
            "/.well-known/openid-configuration".to_string()
        } else {
            format!("/{issuer_path}/.well-known/openid-configuration")
        }
        .as_str(),
    );
    Ok(vec![oauth, oidc])
}

fn validate_public_https_url(value: &str, field: &str) -> Result<Url, ApiError> {
    let url =
        Url::parse(value).map_err(|_| ApiError::bad_request(format!("{field} is invalid")))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::bad_request(format!(
            "{field} must be a public HTTPS URL without credentials or fragment"
        )));
    }
    Ok(url)
}

fn normalize_token_endpoint_auth_method(
    value: Option<&str>,
    has_secret: bool,
) -> Result<String, ApiError> {
    let method = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(if has_secret {
            "client_secret_basic"
        } else {
            "none"
        });
    if !matches!(
        method,
        "none" | "client_secret_basic" | "client_secret_post"
    ) {
        return Err(ApiError::bad_request(
            "OAuth token_endpoint_auth_method is unsupported",
        ));
    }
    if (method == "none") == has_secret {
        return Err(ApiError::bad_request(
            "OAuth client secret does not match token_endpoint_auth_method",
        ));
    }
    Ok(method.to_string())
}

fn require_supported_auth_method(method: &str, supported: &[String]) -> Result<(), ApiError> {
    if supported.is_empty() || supported.iter().any(|value| value == method) {
        Ok(())
    } else {
        Err(ApiError::conflict(format!(
            "OAuth authorization server does not support token endpoint auth method: {method}"
        )))
    }
}

fn validate_bearer_token_type(value: Option<&str>) -> Result<(), ApiError> {
    if value.is_none_or(|value| value.eq_ignore_ascii_case("bearer")) {
        Ok(())
    } else {
        Err(ApiError::conflict(
            "OAuth token endpoint returned an unsupported token type",
        ))
    }
}

fn normalized_token_scopes(
    returned: Option<&str>,
    requested: &[String],
) -> Result<Vec<String>, ApiError> {
    let Some(returned) = returned.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(requested.to_vec());
    };
    let scopes = normalize_scopes(
        returned
            .split_ascii_whitespace()
            .map(str::to_string)
            .collect(),
    )?;
    if scopes != requested {
        return Err(ApiError::conflict(
            "OAuth token response scopes do not exactly match the authorized signed scopes",
        ));
    }
    Ok(scopes)
}

fn oauth_expiry(expires_in: Option<u64>) -> Result<Option<String>, ApiError> {
    let Some(seconds) = expires_in else {
        return Ok(None);
    };
    if seconds == 0 || seconds > 365 * 24 * 60 * 60 {
        return Err(ApiError::conflict("OAuth token expiry is invalid"));
    }
    let seconds =
        i64::try_from(seconds).map_err(|_| ApiError::conflict("OAuth token expiry is invalid"))?;
    Ok(Some(
        (Utc::now() + ChronoDuration::seconds(seconds)).to_rfc3339(),
    ))
}

fn oauth_access_token_needs_refresh(
    connection: &PluginCloudOAuthConnectionRecord,
    required_valid_until_unix: i64,
) -> Result<bool, ApiError> {
    let Some(expires_at) = connection.expires_at.as_deref() else {
        return Ok(false);
    };
    let expiry = DateTime::parse_from_rfc3339(expires_at)
        .map_err(|_| ApiError::conflict("Plugin OAuth expiry is invalid"))?;
    Ok(expiry.timestamp() <= required_valid_until_unix)
}

fn validate_token_secret(value: String, field: &str) -> Result<Zeroizing<String>, ApiError> {
    if value.is_empty() || value.len() > 64 * 1024 || value.chars().any(char::is_control) {
        return Err(ApiError::conflict(format!("{field} is invalid")));
    }
    Ok(Zeroizing::new(value))
}

fn validate_oauth_text(value: &str, field: &str, max_bytes: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if !is_bounded_oauth_text(value, max_bytes) {
        return Err(ApiError::bad_request(format!("{field} is invalid")));
    }
    Ok(value.to_string())
}

fn is_bounded_oauth_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

fn random_secret(bytes: usize) -> String {
    let mut secret = vec![0_u8; bytes];
    rand::fill(secret.as_mut_slice());
    URL_SAFE_NO_PAD.encode(secret)
}

fn sha256_hex(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

fn oauth_authorization_aad(flow_id: &str, state_sha256: &str) -> String {
    format!("chatos.plugin.cloud-oauth-authorization.v1\n{flow_id}\n{state_sha256}")
}

fn normalized_url(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_string()
}

fn callback_origin(public_base_url: &str) -> Result<String, ApiError> {
    let url = Url::parse(public_base_url)
        .map_err(|_| ApiError::internal("Plugin OAuth public base URL is invalid"))?;
    let origin = url.origin().ascii_serialization();
    if origin == "null" {
        return Err(ApiError::internal(
            "Plugin OAuth public base URL does not have an HTTP origin",
        ));
    }
    Ok(origin)
}

fn oauth_callback_response(
    frontend_origin: &str,
    result: Result<PluginCloudOAuthConnectionRecord, String>,
) -> Response {
    let (ok, connection_id, message) = match result {
        Ok(connection) => (
            true,
            Some(connection.id),
            "OAuth authorization completed".to_string(),
        ),
        Err(message) => (false, None, message),
    };
    let payload = serde_json::json!({
        "type": "chatos-plugin-cloud-oauth",
        "ok": ok,
        "connection_id": connection_id,
        "message": message,
    });
    let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    let origin = serde_json::to_string(frontend_origin).unwrap_or_else(|_| "\"\"".to_string());
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>ChatOS OAuth</title></head><body><p>OAuth authorization finished. You may close this window.</p><script>if(window.opener){{window.opener.postMessage({payload},{origin});}}window.close();</script></body></html>"
    );
    let mut response = Html(html).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    response
}

async fn write_oauth_audit(
    state: &AppState,
    event: &str,
    owner_user_id: &str,
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
    outcome: &str,
) -> Result<(), ApiError> {
    let mut audit = plugin_audit_record(
        event,
        owner_user_id,
        None,
        plugin_id,
        Some(release_id),
        outcome,
        BTreeMap::new(),
    );
    audit.component_key = Some(component_key.to_string());
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 224
        || ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])
        || ip.octets()[0] == 198 && matches!(ip.octets()[1], 18 | 19))
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (ip.segments()[0] & 0xfe00) == 0xfc00
        || (ip.segments()[0] & 0xffc0) == 0xfe80
        || (ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_url_uses_pkce_state_resource_and_exact_scopes() {
        let url = build_authorization_url(
            "https://auth.example.com/authorize?ignored=true",
            "client-1",
            "https://plugins.example.com/api/plugins/cloud-oauth/callback",
            "state-1",
            "challenge-1",
            "https://mcp.example.com/mcp",
            &["files:read".to_string(), "files:write".to_string()],
        )
        .unwrap();
        let values = url.query_pairs().collect::<BTreeMap<_, _>>();
        assert_eq!(
            values.get("response_type").map(|value| value.as_ref()),
            Some("code")
        );
        assert_eq!(
            values
                .get("code_challenge_method")
                .map(|value| value.as_ref()),
            Some("S256")
        );
        assert_eq!(
            values.get("resource").map(|value| value.as_ref()),
            Some("https://mcp.example.com/mcp")
        );
        assert_eq!(
            values.get("scope").map(|value| value.as_ref()),
            Some("files:read files:write")
        );
        assert!(!values.contains_key("ignored"));
    }

    #[test]
    fn metadata_locations_follow_resource_and_issuer_paths() {
        let resource = Url::parse("https://mcp.example.com/v1/mcp").unwrap();
        let resource_urls = protected_resource_metadata_urls(&resource).unwrap();
        assert_eq!(
            resource_urls[0].as_str(),
            "https://mcp.example.com/.well-known/oauth-protected-resource/v1/mcp"
        );
        let issuer = Url::parse("https://auth.example.com/tenant").unwrap();
        let issuer_urls = authorization_server_metadata_urls(&issuer).unwrap();
        assert_eq!(
            issuer_urls[0].as_str(),
            "https://auth.example.com/.well-known/oauth-authorization-server/tenant"
        );
        assert_eq!(
            issuer_urls[1].as_str(),
            "https://auth.example.com/tenant/.well-known/openid-configuration"
        );
    }

    #[test]
    fn token_auth_method_requires_matching_secret_shape() {
        assert_eq!(
            normalize_token_endpoint_auth_method(None, false).unwrap(),
            "none"
        );
        assert_eq!(
            normalize_token_endpoint_auth_method(None, true).unwrap(),
            "client_secret_basic"
        );
        assert!(normalize_token_endpoint_auth_method(Some("none"), true).is_err());
        assert!(normalize_token_endpoint_auth_method(Some("client_secret_post"), false).is_err());
    }

    #[test]
    fn returned_oauth_scopes_must_match_the_signed_request_exactly() {
        let requested = vec!["files:read".to_string(), "files:write".to_string()];
        assert_eq!(
            normalized_token_scopes(Some("files:write files:read"), &requested).unwrap(),
            requested
        );
        assert!(normalized_token_scopes(Some("files:read"), &requested).is_err());
        assert!(normalized_token_scopes(Some("files:read files:write admin"), &requested).is_err());
    }

    #[test]
    fn oauth_refresh_window_is_fail_closed_for_invalid_expiry() {
        let mut connection = PluginCloudOAuthConnectionRecord {
            id: "oauth-1".to_string(),
            owner_user_id: "owner-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            component_key: "mcp-1".to_string(),
            provider: "figma".to_string(),
            resource: "https://mcp.example.com/mcp".to_string(),
            scopes: vec!["files:read".to_string()],
            connected: true,
            needs_auth: false,
            refreshable: true,
            expires_at: Some((Utc::now() + ChronoDuration::seconds(30)).to_rfc3339()),
            account_display: None,
            revision: "revision-1".to_string(),
            updated_at: now_rfc3339(),
        };
        assert!(
            oauth_access_token_needs_refresh(&connection, Utc::now().timestamp() + 90).unwrap()
        );
        connection.expires_at = Some("invalid".to_string());
        assert!(
            oauth_access_token_needs_refresh(&connection, Utc::now().timestamp() + 90).is_err()
        );
    }

    #[test]
    fn callback_html_never_contains_oauth_tokens() {
        let response = oauth_callback_response(
            "https://plugins.example.com",
            Err("OAuth authorization failed".to_string()),
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store, max-age=0"
        );
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
    }

    #[test]
    fn private_and_special_addresses_are_rejected() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "192.168.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(value.parse().unwrap()), "{value}");
        }
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }
}
