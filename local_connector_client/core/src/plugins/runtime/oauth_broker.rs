// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chatos_plugin_management_sdk::PluginComponentKind;
use chrono::{DateTime, SecondsFormat, Utc};
use futures_util::StreamExt;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;
use zeroize::Zeroize;

use super::mcp_adapter::load_verified_manifest;
use crate::plugins::{PluginCredentialScope, PluginCredentialVault, PluginInstaller};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OAuthConnectionState {
    schema_version: u32,
    connections: BTreeMap<String, LocalPluginOAuthConnection>,
}

impl Default for OAuthConnectionState {
    fn default() -> Self {
        Self {
            schema_version: OAUTH_STATE_SCHEMA_VERSION,
            connections: BTreeMap::new(),
        }
    }
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

    async fn refresh_connection_if_needed(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
    ) -> Result<LocalPluginOAuthConnection> {
        let storage_key = connection_key_for(expected);
        let refresh_lock = self.refresh_lock(storage_key.as_str())?;
        let _refresh_guard = refresh_lock.lock().await;
        let current = self.load_exact_bound_connection(expected, expected_binding_sha256)?;
        if !connection_needs_refresh(&current)? {
            validate_connection_expiry(&current)?;
            return Ok(current);
        }

        let app = load_oauth_app(
            &self.installer,
            current.plugin_id.as_str(),
            current.release_id.as_str(),
            current.component_key.as_str(),
        )?;
        if app.provider != current.provider || app.resource != current.resource {
            bail!("Plugin OAuth Connected App changed after authorization");
        }
        let refresh_token = match self.resolve_refresh_token(&current) {
            Ok(refresh_token) => refresh_token,
            Err(error) => {
                if validate_connection_expiry(&current).is_ok() {
                    return Ok(current);
                }
                let _ = self.mark_connection_needs_auth(expected, expected_binding_sha256);
                return Err(error)
                    .context("Plugin OAuth access token expired without a refresh token");
            }
        };
        let token = match self
            .exchange_refresh_token(&app, refresh_token.as_str())
            .await
        {
            Ok(token) => token,
            Err(error) => {
                let _ = self.mark_connection_needs_auth(expected, expected_binding_sha256);
                return Err(error).context("refresh Plugin OAuth connection");
            }
        };
        drop(refresh_token);
        match self.persist_refreshed_connection(expected, expected_binding_sha256, &app, token) {
            Ok(connection) => Ok(connection),
            Err(error) => {
                let _ = self.mark_connection_needs_auth(expected, expected_binding_sha256);
                Err(error).context("persist refreshed Plugin OAuth connection")
            }
        }
    }

    fn refresh_lock(&self, storage_key: &str) -> Result<Arc<tokio::sync::Mutex<()>>> {
        let mut locks = self
            .refresh_locks
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin OAuth refresh lock registry is poisoned"))?;
        Ok(locks
            .entry(storage_key.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone())
    }

    fn load_exact_bound_connection(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
    ) -> Result<LocalPluginOAuthConnection> {
        let _guard = self.operation_guard()?;
        let state = load_state(self.state_path.as_path())?;
        let connection = state
            .connections
            .get(connection_key_for(expected).as_str())
            .context("Plugin OAuth connection no longer exists")?;
        self.verify_connection_seal(connection)?;
        if !connection.connected || connection.needs_auth {
            bail!("Plugin OAuth connection requires authorization");
        }
        if connection.id != expected.id
            || connection_binding_sha256(connection) != expected_binding_sha256
        {
            bail!("Plugin OAuth connection changed after prepare");
        }
        Ok(connection.clone())
    }

    fn resolve_refresh_token(
        &self,
        connection: &LocalPluginOAuthConnection,
    ) -> Result<ResolvedOAuthRefreshToken> {
        let scope = token_scope(connection, REFRESH_TOKEN_SECRET)?;
        let handle = self.vault.issue_handle(&scope, TOKEN_HANDLE_TTL)?;
        let resolved = self.vault.resolve_handle(handle.as_str(), &scope);
        let _ = self.vault.revoke_handle(handle.as_str());
        let secret = resolved?;
        let value = std::str::from_utf8(secret.as_bytes())
            .context("Plugin OAuth refresh token is not UTF-8")?
            .to_string();
        Ok(ResolvedOAuthRefreshToken(value))
    }

    fn persist_refreshed_connection(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
        app: &PluginOAuthAppManifest,
        mut token: OAuthTokenResponse,
    ) -> Result<LocalPluginOAuthConnection> {
        let _guard = self.operation_guard()?;
        let mut state = load_state(self.state_path.as_path())?;
        let storage_key = connection_key_for(expected);
        let current = state
            .connections
            .get(storage_key.as_str())
            .context("Plugin OAuth connection no longer exists")?;
        self.verify_connection_seal(current)?;
        if !current.connected
            || current.needs_auth
            || current.id != expected.id
            || connection_binding_sha256(current) != expected_binding_sha256
        {
            zeroize_token_response(&mut token);
            bail!("Plugin OAuth connection changed while refreshing");
        }
        let scopes = match refreshed_scopes(current, app, token.scope.as_deref()) {
            Ok(scopes) => scopes,
            Err(error) => {
                zeroize_token_response(&mut token);
                return Err(error);
            }
        };
        let Some(expires_at) = token_expiry(token.expires_in) else {
            zeroize_token_response(&mut token);
            bail!("Plugin OAuth refresh response omitted expires_in");
        };
        let mut connection = current.clone();
        connection.scopes = scopes;
        connection.connected = true;
        connection.needs_auth = false;
        connection.expires_at = Some(expires_at);
        connection.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

        let access_scope = token_scope(&connection, ACCESS_TOKEN_SECRET)?;
        if let Err(error) = self
            .vault
            .upsert(&access_scope, token.access_token.as_bytes())
        {
            zeroize_token_response(&mut token);
            return Err(error);
        }
        token.access_token.zeroize();
        if let Some(mut refresh_token) = token.refresh_token.take() {
            let refresh_scope = token_scope(&connection, REFRESH_TOKEN_SECRET)?;
            if let Err(error) = self.vault.upsert(&refresh_scope, refresh_token.as_bytes()) {
                refresh_token.zeroize();
                return Err(error);
            }
            refresh_token.zeroize();
        }
        let seal_scope = token_scope(&connection, CONNECTION_SEAL_SECRET)?;
        self.vault.upsert(
            &seal_scope,
            connection_snapshot_sha256(&connection).as_bytes(),
        )?;
        state.connections.insert(storage_key, connection.clone());
        save_state(self.state_path.as_path(), &state)?;
        Ok(connection)
    }

    fn mark_connection_needs_auth(
        &self,
        expected: &LocalPluginOAuthConnection,
        expected_binding_sha256: &str,
    ) -> Result<()> {
        let _guard = self.operation_guard()?;
        let mut state = load_state(self.state_path.as_path())?;
        let storage_key = connection_key_for(expected);
        let Some(current) = state.connections.get(storage_key.as_str()) else {
            return Ok(());
        };
        self.verify_connection_seal(current)?;
        if current.id != expected.id
            || connection_binding_sha256(current) != expected_binding_sha256
        {
            return Ok(());
        }
        let mut connection = current.clone();
        connection.connected = false;
        connection.needs_auth = true;
        connection.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let _ = self
            .vault
            .delete(&token_scope(&connection, ACCESS_TOKEN_SECRET)?)?;
        let _ = self
            .vault
            .delete(&token_scope(&connection, REFRESH_TOKEN_SECRET)?)?;
        self.vault.upsert(
            &token_scope(&connection, CONNECTION_SEAL_SECRET)?,
            connection_snapshot_sha256(&connection).as_bytes(),
        )?;
        state.connections.insert(storage_key, connection);
        save_state(self.state_path.as_path(), &state)
    }

    fn verify_connection_seal(&self, connection: &LocalPluginOAuthConnection) -> Result<()> {
        let scope = token_scope(connection, CONNECTION_SEAL_SECRET)?;
        let handle = self.vault.issue_handle(&scope, TOKEN_HANDLE_TTL)?;
        let resolved = self.vault.resolve_handle(handle.as_str(), &scope);
        let _ = self.vault.revoke_handle(handle.as_str());
        let seal = resolved?;
        if seal.as_bytes() != connection_snapshot_sha256(connection).as_bytes() {
            bail!("Plugin OAuth connection metadata seal mismatch");
        }
        Ok(())
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

fn load_oauth_app(
    installer: &PluginInstaller,
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
) -> Result<PluginOAuthAppManifest> {
    let installation = installer
        .active_installation(plugin_id)?
        .context("Plugin is not installed and active")?;
    if installation.version.release_id != release_id {
        bail!("Plugin OAuth request does not match the active Release");
    }
    let component = installation
        .version
        .inventory
        .components
        .iter()
        .find(|component| component.component_key == component_key)
        .context("Plugin OAuth component is not in the signed inventory")?;
    if component.kind != PluginComponentKind::ConnectedApp {
        bail!("Plugin OAuth component is not a Connected App");
    }
    let manifest = load_verified_manifest(&installation)?;
    let app = manifest
        .apps
        .iter()
        .find(|app| app.component_key == component_key)
        .context("Plugin Connected App is not in the normalized Manifest")?;
    let relative_path = app.manifest.path.trim_start_matches("./");
    let expected = installation
        .version
        .package_file_sha256
        .get(relative_path)
        .context("Plugin Connected App manifest is not covered by checksums")?;
    let path = installation.installation_path.join(relative_path);
    let metadata = fs::symlink_metadata(path.as_path())
        .context("read Plugin Connected App manifest metadata")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_OAUTH_APP_BYTES
    {
        bail!("Plugin Connected App manifest is unsafe or oversized");
    }
    let bytes = fs::read(path.as_path()).context("read Plugin Connected App manifest")?;
    if hex::encode(Sha256::digest(bytes.as_slice())) != *expected {
        bail!("Plugin Connected App manifest checksum mismatch");
    }
    let mut app: PluginOAuthAppManifest =
        serde_json::from_slice(bytes.as_slice()).context("parse Plugin Connected App manifest")?;
    validate_oauth_app(&mut app)?;
    Ok(app)
}

fn validate_oauth_app(app: &mut PluginOAuthAppManifest) -> Result<()> {
    if app.schema_version != OAUTH_APP_SCHEMA_VERSION {
        bail!("unsupported Plugin Connected App schema version");
    }
    app.provider = validate_identifier("OAuth provider", app.provider.as_str(), 96)?;
    app.client_id = required_value("OAuth client id", app.client_id.as_str(), 512)?;
    app.resource = required_value("OAuth resource", app.resource.as_str(), 2048)?;
    app.scopes = normalize_scopes(std::mem::take(&mut app.scopes))?;
    if app.callback_type != "loopback" {
        bail!("Plugin OAuth callback type must be loopback");
    }
    validate_oauth_endpoint(app.authorization_url.as_str())?;
    validate_oauth_endpoint(app.token_url.as_str())?;
    for reserved in [
        "response_type",
        "client_id",
        "redirect_uri",
        "state",
        "scope",
        "code_challenge",
        "code_challenge_method",
    ] {
        if app.authorization_params.contains_key(reserved) {
            bail!("Plugin OAuth authorization params cannot override {reserved}");
        }
    }
    if app.authorization_params.len() > 32 {
        bail!("Plugin OAuth authorization params exceed the count limit");
    }
    for (key, value) in &app.authorization_params {
        validate_identifier("OAuth authorization parameter", key, 96)?;
        required_value("OAuth authorization parameter value", value, 2048)?;
    }
    Ok(())
}

fn validate_oauth_endpoint(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("parse Plugin OAuth endpoint")?;
    let host = url
        .host_str()
        .context("Plugin OAuth endpoint is missing host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .ok()
            .is_some_and(|address| address.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        bail!("Plugin OAuth endpoints require HTTPS except for loopback development providers");
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        bail!("Plugin OAuth endpoints cannot contain credentials or URL fragments");
    }
    Ok(())
}

fn validate_loopback_redirect_uri(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("parse Plugin OAuth redirect URI")?;
    let host = url
        .host_str()
        .context("Plugin OAuth redirect URI is missing host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .ok()
            .is_some_and(|address| address.is_loopback());
    if url.scheme() != "http" || !loopback || url.port().is_none() {
        bail!("Plugin OAuth redirect URI must be an explicit loopback HTTP port");
    }
    Ok(())
}

fn normalize_scopes(values: Vec<String>) -> Result<Vec<String>> {
    let mut scopes = BTreeSet::new();
    for value in values {
        let value = required_value("OAuth scope", value.as_str(), 256)?;
        if !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b'+' | b'=')
        }) {
            bail!("OAuth scope contains unsupported characters");
        }
        scopes.insert(value);
    }
    if scopes.len() > 64 {
        bail!("Plugin OAuth scopes exceed the count limit");
    }
    Ok(scopes.into_iter().collect())
}

fn validate_token_response(token: &OAuthTokenResponse) -> Result<()> {
    if token.access_token.is_empty() || token.access_token.len() > 64 * 1024 {
        bail!("Plugin OAuth token endpoint returned an invalid access token");
    }
    if token
        .token_type
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case("bearer"))
    {
        bail!("Plugin OAuth only supports Bearer access tokens");
    }
    if token
        .refresh_token
        .as_ref()
        .is_some_and(|value| value.is_empty() || value.len() > 64 * 1024)
    {
        bail!("Plugin OAuth token endpoint returned an invalid refresh token");
    }
    Ok(())
}

fn zeroize_token_response(token: &mut OAuthTokenResponse) {
    token.access_token.zeroize();
    if let Some(refresh_token) = &mut token.refresh_token {
        refresh_token.zeroize();
    }
}

fn refreshed_scopes(
    current: &LocalPluginOAuthConnection,
    app: &PluginOAuthAppManifest,
    response_scope: Option<&str>,
) -> Result<Vec<String>> {
    let scopes = match response_scope {
        Some(value) => normalize_scopes(value.split_whitespace().map(str::to_string).collect())?,
        None => current.scopes.clone(),
    };
    if !app.scopes.iter().all(|scope| scopes.contains(scope)) {
        bail!("Plugin OAuth refresh response omitted required scopes");
    }
    if scopes != current.scopes {
        bail!("Plugin OAuth refresh response changed the authorized scopes");
    }
    Ok(scopes)
}

fn token_expiry(expires_in: Option<u64>) -> Option<String> {
    expires_in.map(|seconds| {
        let expires_at =
            Utc::now() + chrono::Duration::seconds(seconds.clamp(60, 365 * 24 * 60 * 60) as i64);
        expires_at.to_rfc3339_opts(SecondsFormat::Millis, true)
    })
}

fn validate_identifier(label: &str, value: &str, max_bytes: usize) -> Result<String> {
    let value = required_value(label, value, max_bytes)?.to_ascii_lowercase();
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
    }) {
        bail!("{label} contains unsupported characters");
    }
    Ok(value)
}

fn required_value(label: &str, value: &str, max_bytes: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        bail!("{label} is missing or invalid");
    }
    Ok(value.to_string())
}

fn validate_callback_error(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        bail!("Plugin OAuth callback error is invalid");
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_callback_error_description(value: &str) -> Result<String> {
    required_value("Plugin OAuth callback error description", value, 1024)
}

fn random_urlsafe(bytes: usize) -> Result<String> {
    let mut value = vec![0_u8; bytes];
    SystemRandom::new()
        .fill(value.as_mut_slice())
        .map_err(|_| anyhow::anyhow!("generate Plugin OAuth random value failed"))?;
    Ok(URL_SAFE_NO_PAD.encode(value))
}

fn prune_pending(pending: &mut HashMap<String, PendingOAuthTransaction>) {
    let now = Utc::now().timestamp();
    pending.retain(|_, transaction| {
        let active = transaction.expires_at > now;
        if !active {
            transaction.code_verifier.zeroize();
        }
        active
    });
}

fn timestamp_rfc3339(timestamp: i64) -> Result<String> {
    DateTime::from_timestamp(timestamp, 0)
        .context("format Plugin OAuth timestamp")
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn token_scope(
    connection: &LocalPluginOAuthConnection,
    secret_name: &str,
) -> Result<PluginCredentialScope> {
    PluginCredentialScope::new(
        connection.owner_user_id.clone(),
        connection.device_id.clone(),
        connection.plugin_id.clone(),
        connection.release_id.clone(),
        connection.component_key.clone(),
        secret_name.to_string(),
    )
}

fn validate_connection_expiry(connection: &LocalPluginOAuthConnection) -> Result<()> {
    if let Some(expires_at) = connection.expires_at.as_deref() {
        let expires_at = DateTime::parse_from_rfc3339(expires_at)
            .context("parse Plugin OAuth connection expiry")?;
        if expires_at.timestamp() <= Utc::now().timestamp() {
            bail!("Plugin OAuth access token is expired");
        }
    }
    Ok(())
}

fn connection_needs_refresh(connection: &LocalPluginOAuthConnection) -> Result<bool> {
    let Some(expires_at) = connection.expires_at.as_deref() else {
        return Ok(false);
    };
    let expires_at =
        DateTime::parse_from_rfc3339(expires_at).context("parse Plugin OAuth connection expiry")?;
    Ok(expires_at.timestamp() <= Utc::now().timestamp() + OAUTH_REFRESH_EARLY_SECONDS)
}

fn connection_snapshot_sha256(connection: &LocalPluginOAuthConnection) -> String {
    let mut payload = format!(
        "chatos.plugin.oauth.connection.v2\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        connection.id,
        connection.owner_user_id,
        connection.device_id,
        connection.plugin_id,
        connection.release_id,
        connection.component_key,
        connection.provider,
        connection.resource,
        connection.connected,
        connection.needs_auth,
        connection.expires_at.as_deref().unwrap_or_default(),
        connection.account_display.as_deref().unwrap_or_default(),
        connection.updated_at,
    );
    for scope in &connection.scopes {
        payload.push('\n');
        payload.push_str(scope);
    }
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn connection_binding_sha256(connection: &LocalPluginOAuthConnection) -> String {
    let mut payload = format!(
        "chatos.plugin.oauth.binding.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        connection.id,
        connection.owner_user_id,
        connection.device_id,
        connection.plugin_id,
        connection.release_id,
        connection.component_key,
        connection.provider,
        connection.resource,
    );
    for scope in &connection.scopes {
        payload.push('\n');
        payload.push_str(scope);
    }
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn connection_key_for(connection: &LocalPluginOAuthConnection) -> String {
    connection_storage_key(
        connection.owner_user_id.as_str(),
        connection.device_id.as_str(),
        connection.plugin_id.as_str(),
        connection.component_key.as_str(),
        connection.provider.as_str(),
    )
}

fn connection_storage_key(
    owner_user_id: &str,
    device_id: &str,
    plugin_id: &str,
    component_key: &str,
    provider: &str,
) -> String {
    let payload = format!(
        "chatos.plugin.oauth.storage.v1\n{owner_user_id}\n{device_id}\n{plugin_id}\n{component_key}\n{provider}"
    );
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn load_state(path: &Path) -> Result<OAuthConnectionState> {
    if !path.exists() {
        return Ok(OAuthConnectionState::default());
    }
    let metadata = fs::metadata(path).context("read Plugin OAuth state metadata")?;
    if !metadata.is_file() || metadata.len() > MAX_OAUTH_STATE_BYTES {
        bail!("Plugin OAuth state is invalid or oversized");
    }
    let state: OAuthConnectionState =
        serde_json::from_slice(fs::read(path)?.as_slice()).context("parse Plugin OAuth state")?;
    if state.schema_version != OAUTH_STATE_SCHEMA_VERSION {
        bail!("unsupported Plugin OAuth state schema version");
    }
    Ok(state)
}

fn save_state(path: &Path, state: &OAuthConnectionState) -> Result<()> {
    let parent = path.parent().context("Plugin OAuth state has no parent")?;
    fs::create_dir_all(parent).context("create Plugin OAuth state directory")?;
    let payload = serde_json::to_vec_pretty(state).context("serialize Plugin OAuth state")?;
    if payload.len() as u64 > MAX_OAUTH_STATE_BYTES {
        bail!("Plugin OAuth state exceeds the size limit");
    }
    let mut temp = NamedTempFile::new_in(parent).context("create Plugin OAuth state temp file")?;
    temp.write_all(payload.as_slice())?;
    temp.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temp.persist(path)
        .map_err(|error| anyhow::anyhow!("persist Plugin OAuth state: {}", error.error))?;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)
        .context("open Plugin OAuth state directory")?
        .sync_all()
        .context("sync Plugin OAuth state directory")
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}
