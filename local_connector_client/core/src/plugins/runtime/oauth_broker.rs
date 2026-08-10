// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroize;

use super::mcp_runtime::load_verified_manifest;
use crate::plugins::{PluginCredentialVault, PluginInstaller};

mod refresh;
mod support;

use support::{
    connection_binding_sha256, connection_key_for, connection_needs_refresh,
    connection_snapshot_sha256, connection_storage_key, load_oauth_app, load_state,
    normalize_scopes, prune_pending, random_urlsafe, refreshed_scopes, save_state,
    timestamp_rfc3339, token_expiry, token_scope, validate_callback_error,
    validate_callback_error_description, validate_connection_expiry,
    validate_loopback_redirect_uri, validate_token_response, zeroize_token_response,
};

const OAUTH_STATE_SCHEMA_VERSION: u32 = 1;
const OAUTH_APP_SCHEMA_VERSION: u32 = 1;
const MAX_OAUTH_STATE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_OAUTH_APP_BYTES: u64 = 256 * 1024;
const MAX_TOKEN_RESPONSE_BYTES: usize = 1024 * 1024;
const OAUTH_TRANSACTION_TTL_SECONDS: i64 = 10 * 60;
const OAUTH_REFRESH_EARLY_SECONDS: i64 = 5 * 60;
const TOKEN_HANDLE_TTL: Duration = Duration::from_secs(60);
const ACCESS_TOKEN_SECRET: &str = "oauth.access_token";
const REFRESH_TOKEN_SECRET: &str = "oauth.refresh_token";
const CONNECTION_SEAL_SECRET: &str = "oauth.connection_snapshot";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginOAuthAppManifest {
    pub schema_version: u32,
    pub provider: String,
    pub client_id: String,
    pub authorization_url: String,
    pub token_url: String,
    pub resource: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub callback_type: String,
    #[serde(default)]
    pub authorization_params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginOAuthAuthorizationStart {
    pub transaction_id: String,
    pub authorization_url: String,
    pub expires_at: String,
    pub browser_opened: bool,
    pub browser_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginOAuthAuthorizationFailure {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPluginOAuthConnection {
    pub id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub provider: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub connected: bool,
    #[serde(default)]
    pub needs_auth: bool,
    pub expires_at: Option<String>,
    pub account_display: Option<String>,
    pub updated_at: String,
}

#[derive(Clone)]
struct PendingOAuthTransaction {
    transaction_id: String,
    owner_user_id: String,
    device_id: String,
    plugin_id: String,
    release_id: String,
    component_key: String,
    redirect_uri: String,
    code_verifier: String,
    app: PluginOAuthAppManifest,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
}

#[derive(Clone)]
pub struct PluginOAuthBroker {
    installer: PluginInstaller,
    vault: PluginCredentialVault,
    state_path: PathBuf,
    http_client: Result<reqwest::Client, String>,
    operation_lock: Arc<Mutex<()>>,
    pending: Arc<Mutex<HashMap<String, PendingOAuthTransaction>>>,
    refresh_locks: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl std::fmt::Debug for PluginOAuthBroker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginOAuthBroker")
            .field("plugin_root", &self.installer.plugin_root())
            .field("state_path", &self.state_path)
            .finish_non_exhaustive()
    }
}

impl PluginOAuthBroker {
    pub fn new(installer: PluginInstaller, vault: PluginCredentialVault) -> Self {
        Self {
            state_path: installer.plugin_root().join("oauth-connections.json"),
            installer,
            vault,
            http_client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| error.to_string()),
            operation_lock: Arc::new(Mutex::new(())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            refresh_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn list_connections(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
    ) -> Result<Vec<LocalPluginOAuthConnection>> {
        let _guard = self.operation_guard()?;
        let active_release_id = self
            .installer
            .active_installation(plugin_id)?
            .map(|installation| installation.version.release_id);
        let mut state = load_state(self.state_path.as_path())?;
        let before = state.connections.len();
        state.connections.retain(|_, connection| {
            connection.plugin_id != plugin_id
                || active_release_id.as_deref() == Some(connection.release_id.as_str())
        });
        if state.connections.len() != before {
            save_state(self.state_path.as_path(), &state)?;
        }
        let mut connections = state
            .connections
            .into_values()
            .filter(|connection| {
                connection.owner_user_id == owner_user_id
                    && connection.device_id == device_id
                    && connection.plugin_id == plugin_id
            })
            .collect::<Vec<_>>();
        for connection in &connections {
            self.verify_connection_seal(connection)?;
        }
        connections.sort_by(|left, right| {
            left.component_key
                .cmp(&right.component_key)
                .then_with(|| left.provider.cmp(&right.provider))
        });
        Ok(connections)
    }

    pub(crate) fn status_connections(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<Vec<LocalPluginOAuthConnection>> {
        let _guard = self.operation_guard()?;
        let mut connections = Vec::new();
        for connection in load_state(self.state_path.as_path())?
            .connections
            .into_values()
        {
            if connection.owner_user_id != owner_user_id || connection.device_id != device_id {
                continue;
            }
            let active_release_id = self
                .installer
                .active_installation(connection.plugin_id.as_str())?
                .map(|installation| installation.version.release_id);
            if active_release_id.as_deref() != Some(connection.release_id.as_str()) {
                continue;
            }
            self.verify_connection_seal(&connection)?;
            connections.push(connection);
        }
        connections.sort_by(|left, right| {
            left.plugin_id
                .cmp(&right.plugin_id)
                .then_with(|| left.component_key.cmp(&right.component_key))
                .then_with(|| left.provider.cmp(&right.provider))
        });
        Ok(connections)
    }

    pub fn begin_authorization(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
        release_id: &str,
        component_key: &str,
        redirect_uri: &str,
    ) -> Result<PluginOAuthAuthorizationStart> {
        validate_loopback_redirect_uri(redirect_uri)?;
        let app = load_oauth_app(&self.installer, plugin_id, release_id, component_key)?;
        let state = random_urlsafe(32)?;
        let code_verifier = random_urlsafe(48)?;
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let transaction_id = Uuid::new_v4().to_string();
        let expires_at = Utc::now().timestamp() + OAUTH_TRANSACTION_TTL_SECONDS;
        let mut authorization_url = reqwest::Url::parse(app.authorization_url.as_str())
            .context("parse Plugin OAuth authorization URL")?;
        {
            let mut query = authorization_url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", app.client_id.as_str());
            query.append_pair("redirect_uri", redirect_uri);
            query.append_pair("state", state.as_str());
            query.append_pair("code_challenge", code_challenge.as_str());
            query.append_pair("code_challenge_method", "S256");
            if !app.scopes.is_empty() {
                query.append_pair("scope", app.scopes.join(" ").as_str());
            }
            for (key, value) in &app.authorization_params {
                query.append_pair(key, value);
            }
        }
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin OAuth transaction lock is poisoned"))?;
        prune_pending(&mut pending);
        if let Some(mut replaced) = pending.insert(
            state,
            PendingOAuthTransaction {
                transaction_id: transaction_id.clone(),
                owner_user_id: owner_user_id.to_string(),
                device_id: device_id.to_string(),
                plugin_id: plugin_id.to_string(),
                release_id: release_id.to_string(),
                component_key: component_key.to_string(),
                redirect_uri: redirect_uri.to_string(),
                code_verifier,
                app,
                expires_at,
            },
        ) {
            replaced.code_verifier.zeroize();
        }
        Ok(PluginOAuthAuthorizationStart {
            transaction_id,
            authorization_url: authorization_url.to_string(),
            expires_at: timestamp_rfc3339(expires_at)?,
            browser_opened: false,
            browser_error: None,
        })
    }

    pub async fn complete_authorization(
        &self,
        state: &str,
        code: &str,
    ) -> Result<LocalPluginOAuthConnection> {
        if state.trim().is_empty() || code.trim().is_empty() {
            bail!("Plugin OAuth callback requires state and code");
        }
        let mut transaction = {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| anyhow::anyhow!("Plugin OAuth transaction lock is poisoned"))?;
            prune_pending(&mut pending);
            pending
                .remove(state)
                .context("Plugin OAuth state is invalid or expired")?
        };
        let token = self.exchange_code(&transaction, code).await;
        transaction.code_verifier.zeroize();
        self.persist_connection(&transaction, token?)
    }

    pub(crate) fn consume_authorization_error(
        &self,
        state: &str,
        error: &str,
        error_description: Option<&str>,
    ) -> Result<PluginOAuthAuthorizationFailure> {
        if state.trim().is_empty() {
            bail!("Plugin OAuth callback requires state");
        }
        let mut transaction = {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| anyhow::anyhow!("Plugin OAuth transaction lock is poisoned"))?;
            prune_pending(&mut pending);
            pending
                .remove(state)
                .context("Plugin OAuth state is invalid or expired")?
        };
        transaction.code_verifier.zeroize();
        let error = validate_callback_error(error)?;
        let description = error_description
            .map(validate_callback_error_description)
            .transpose()?;
        let code = if error == "access_denied" {
            "plugin_oauth_access_denied"
        } else {
            "plugin_oauth_authorization_failed"
        };
        let mut message = if error == "access_denied" {
            "Plugin OAuth authorization was denied".to_string()
        } else {
            format!("Plugin OAuth authorization failed: {error}")
        };
        if let Some(description) = description {
            message.push_str(" (");
            message.push_str(description.as_str());
            message.push(')');
        }
        Ok(PluginOAuthAuthorizationFailure { code, message })
    }

    pub fn disconnect(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
        component_key: &str,
        provider: &str,
    ) -> Result<bool> {
        let _guard = self.operation_guard()?;
        let mut state = load_state(self.state_path.as_path())?;
        let connection_id =
            connection_storage_key(owner_user_id, device_id, plugin_id, component_key, provider);
        let Some(mut connection) = state.connections.get(connection_id.as_str()).cloned() else {
            return Ok(false);
        };
        self.verify_connection_seal(&connection)?;
        for secret_name in [ACCESS_TOKEN_SECRET, REFRESH_TOKEN_SECRET] {
            let scope = token_scope(&connection, secret_name)?;
            let _ = self.vault.delete(&scope)?;
        }
        connection.connected = false;
        connection.needs_auth = false;
        connection.expires_at = None;
        connection.account_display = None;
        connection.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        self.vault.upsert(
            &token_scope(&connection, CONNECTION_SEAL_SECRET)?,
            connection_snapshot_sha256(&connection).as_bytes(),
        )?;
        state.connections.insert(connection_id, connection);
        save_state(self.state_path.as_path(), &state)?;
        Ok(true)
    }

    async fn exchange_code(
        &self,
        transaction: &PendingOAuthTransaction,
        code: &str,
    ) -> Result<OAuthTokenResponse> {
        self.request_token(
            transaction.app.token_url.as_str(),
            &[
                ("grant_type", "authorization_code"),
                ("client_id", transaction.app.client_id.as_str()),
                ("code", code),
                ("redirect_uri", transaction.redirect_uri.as_str()),
                ("code_verifier", transaction.code_verifier.as_str()),
            ],
            "exchange Plugin OAuth authorization code",
        )
        .await
    }

    async fn exchange_refresh_token(
        &self,
        app: &PluginOAuthAppManifest,
        refresh_token: &str,
    ) -> Result<OAuthTokenResponse> {
        self.request_token(
            app.token_url.as_str(),
            &[
                ("grant_type", "refresh_token"),
                ("client_id", app.client_id.as_str()),
                ("refresh_token", refresh_token),
            ],
            "refresh Plugin OAuth access token",
        )
        .await
    }

    async fn request_token(
        &self,
        token_url: &str,
        form: &[(&str, &str)],
        request_context: &'static str,
    ) -> Result<OAuthTokenResponse> {
        let client = self
            .http_client
            .as_ref()
            .map_err(|error| anyhow::anyhow!("Plugin OAuth HTTP client is unavailable: {error}"))?;
        let response = client
            .post(token_url)
            .form(form)
            .send()
            .await
            .context(request_context)?;
        if !response.status().is_success() {
            bail!(
                "Plugin OAuth token endpoint returned HTTP {}",
                response.status()
            );
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_TOKEN_RESPONSE_BYTES as u64)
        {
            bail!("Plugin OAuth token response exceeds the size limit");
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("read Plugin OAuth token response")?;
            if bytes.len().saturating_add(chunk.len()) > MAX_TOKEN_RESPONSE_BYTES {
                bytes.zeroize();
                bail!("Plugin OAuth token response exceeds the size limit");
            }
            bytes.extend_from_slice(chunk.as_ref());
        }
        let parsed =
            serde_json::from_slice(bytes.as_slice()).context("parse Plugin OAuth token response");
        bytes.zeroize();
        let mut token: OAuthTokenResponse = parsed?;
        if let Err(error) = validate_token_response(&token) {
            token.access_token.zeroize();
            if let Some(refresh_token) = &mut token.refresh_token {
                refresh_token.zeroize();
            }
            return Err(error);
        }
        Ok(token)
    }

    fn persist_connection(
        &self,
        transaction: &PendingOAuthTransaction,
        mut token: OAuthTokenResponse,
    ) -> Result<LocalPluginOAuthConnection> {
        let _guard = self.operation_guard()?;
        let installation = self
            .installer
            .active_installation(transaction.plugin_id.as_str())?
            .context("Plugin is no longer installed and active")?;
        if installation.version.release_id != transaction.release_id {
            bail!("Plugin OAuth transaction Release is no longer active");
        }
        let scopes = normalize_scopes(
            token
                .scope
                .as_deref()
                .map(|value| value.split_whitespace().map(str::to_string).collect())
                .unwrap_or_else(|| transaction.app.scopes.clone()),
        )?;
        if !transaction
            .app
            .scopes
            .iter()
            .all(|scope| scopes.contains(scope))
        {
            token.access_token.zeroize();
            if let Some(refresh_token) = &mut token.refresh_token {
                refresh_token.zeroize();
            }
            bail!("Plugin OAuth token response omitted required scopes");
        }
        let expires_at = token_expiry(token.expires_in);
        let connection = LocalPluginOAuthConnection {
            id: transaction.transaction_id.clone(),
            owner_user_id: transaction.owner_user_id.clone(),
            device_id: transaction.device_id.clone(),
            plugin_id: transaction.plugin_id.clone(),
            release_id: transaction.release_id.clone(),
            component_key: transaction.component_key.clone(),
            provider: transaction.app.provider.clone(),
            resource: transaction.app.resource.clone(),
            scopes,
            connected: true,
            needs_auth: false,
            expires_at,
            account_display: None,
            updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };
        let access_scope = token_scope(&connection, ACCESS_TOKEN_SECRET)?;
        self.vault
            .upsert(&access_scope, token.access_token.as_bytes())?;
        token.access_token.zeroize();
        if let Some(mut refresh_token) = token.refresh_token.take() {
            let refresh_scope = token_scope(&connection, REFRESH_TOKEN_SECRET)?;
            if let Err(error) = self.vault.upsert(&refresh_scope, refresh_token.as_bytes()) {
                refresh_token.zeroize();
                let _ = self.vault.delete(&access_scope);
                return Err(error);
            }
            refresh_token.zeroize();
        }
        let seal_scope = token_scope(&connection, CONNECTION_SEAL_SECRET)?;
        let seal = connection_snapshot_sha256(&connection);
        if let Err(error) = self.vault.upsert(&seal_scope, seal.as_bytes()) {
            let _ = self.vault.delete(&access_scope);
            let _ = self
                .vault
                .delete(&token_scope(&connection, REFRESH_TOKEN_SECRET)?);
            return Err(error);
        }
        let mut state = load_state(self.state_path.as_path())?;
        let storage_key = connection_storage_key(
            connection.owner_user_id.as_str(),
            connection.device_id.as_str(),
            connection.plugin_id.as_str(),
            connection.component_key.as_str(),
            connection.provider.as_str(),
        );
        state.connections.insert(storage_key, connection.clone());
        if let Err(error) = save_state(self.state_path.as_path(), &state) {
            let _ = self.vault.delete(&access_scope);
            let _ = self
                .vault
                .delete(&token_scope(&connection, REFRESH_TOKEN_SECRET)?);
            let _ = self.vault.delete(&seal_scope);
            return Err(error);
        }
        Ok(connection)
    }

    fn operation_guard(&self) -> Result<MutexGuard<'_, ()>> {
        self.operation_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin OAuth operation lock is poisoned"))
    }

    pub(super) fn prepare_token_binding(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
        release_id: &str,
        resource: &str,
    ) -> Result<PluginOAuthTokenBinding> {
        let _guard = self.operation_guard()?;
        let matches = load_state(self.state_path.as_path())?
            .connections
            .into_values()
            .filter(|connection| {
                connection.connected
                    && !connection.needs_auth
                    && connection.owner_user_id == owner_user_id
                    && connection.device_id == device_id
                    && connection.plugin_id == plugin_id
                    && connection.release_id == release_id
                    && connection.resource == resource
            })
            .collect::<Vec<_>>();
        let [connection] = matches.as_slice() else {
            bail!("Plugin OAuth resource requires exactly one active local connection");
        };
        self.verify_connection_seal(connection)?;
        Ok(PluginOAuthTokenBinding {
            broker: self.clone(),
            connection: connection.clone(),
            snapshot_sha256: connection_binding_sha256(connection),
        })
    }
}

#[derive(Clone)]
pub(super) struct PluginOAuthTokenBinding {
    broker: PluginOAuthBroker,
    connection: LocalPluginOAuthConnection,
    snapshot_sha256: String,
}

impl std::fmt::Debug for PluginOAuthTokenBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginOAuthTokenBinding")
            .field("connection_id", &self.connection.id)
            .field("provider", &self.connection.provider)
            .field("resource", &self.connection.resource)
            .field("snapshot_sha256", &self.snapshot_sha256)
            .finish_non_exhaustive()
    }
}

impl PluginOAuthTokenBinding {
    pub(super) fn snapshot_sha256(&self) -> &str {
        self.snapshot_sha256.as_str()
    }

    pub(super) fn provider(&self) -> &str {
        self.connection.provider.as_str()
    }

    pub(super) fn scopes(&self) -> &[String] {
        self.connection.scopes.as_slice()
    }

    pub(super) fn connection_id(&self) -> &str {
        self.connection.id.as_str()
    }

    pub(super) fn verify(&self) -> Result<()> {
        let current = self.broker.prepare_token_binding(
            self.connection.owner_user_id.as_str(),
            self.connection.device_id.as_str(),
            self.connection.plugin_id.as_str(),
            self.connection.release_id.as_str(),
            self.connection.resource.as_str(),
        )?;
        if current.snapshot_sha256 != self.snapshot_sha256
            || current.connection.id != self.connection.id
        {
            bail!("Plugin OAuth connection changed after prepare");
        }
        Ok(())
    }

    pub(super) async fn resolve(&self) -> Result<ResolvedOAuthAccessToken> {
        let connection = self
            .broker
            .refresh_connection_if_needed(&self.connection, self.snapshot_sha256.as_str())
            .await?;
        let scope = token_scope(&connection, ACCESS_TOKEN_SECRET)?;
        let handle = self.broker.vault.issue_handle(&scope, TOKEN_HANDLE_TTL)?;
        let resolved = self.broker.vault.resolve_handle(handle.as_str(), &scope);
        let _ = self.broker.vault.revoke_handle(handle.as_str());
        let secret = resolved?;
        let value = std::str::from_utf8(secret.as_bytes())
            .context("Plugin OAuth access token is not UTF-8")?
            .to_string();
        Ok(ResolvedOAuthAccessToken(value))
    }
}

struct ResolvedOAuthRefreshToken(String);

impl ResolvedOAuthRefreshToken {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Debug for ResolvedOAuthRefreshToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResolvedOAuthRefreshToken([REDACTED])")
    }
}

impl Drop for ResolvedOAuthRefreshToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(super) struct ResolvedOAuthAccessToken(String);

impl ResolvedOAuthAccessToken {
    pub(super) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Debug for ResolvedOAuthAccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResolvedOAuthAccessToken([REDACTED])")
    }
}

impl Drop for ResolvedOAuthAccessToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
