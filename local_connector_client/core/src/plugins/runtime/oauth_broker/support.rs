// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chatos_plugin_management_sdk::PluginComponentKind;
use chrono::{DateTime, SecondsFormat, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use zeroize::Zeroize;

use super::{
    load_verified_manifest, LocalPluginOAuthConnection, OAuthTokenResponse,
    PendingOAuthTransaction, PluginOAuthAppManifest, MAX_OAUTH_APP_BYTES, MAX_OAUTH_STATE_BYTES,
    OAUTH_APP_SCHEMA_VERSION, OAUTH_REFRESH_EARLY_SECONDS, OAUTH_STATE_SCHEMA_VERSION,
};
use crate::plugins::{PluginCredentialScope, PluginInstaller};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OAuthConnectionState {
    pub(super) schema_version: u32,
    pub(super) connections: BTreeMap<String, LocalPluginOAuthConnection>,
}

impl Default for OAuthConnectionState {
    fn default() -> Self {
        Self {
            schema_version: OAUTH_STATE_SCHEMA_VERSION,
            connections: BTreeMap::new(),
        }
    }
}

pub(super) fn load_oauth_app(
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

pub(super) fn validate_loopback_redirect_uri(value: &str) -> Result<()> {
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

pub(super) fn normalize_scopes(values: Vec<String>) -> Result<Vec<String>> {
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

pub(super) fn validate_token_response(token: &OAuthTokenResponse) -> Result<()> {
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

pub(super) fn zeroize_token_response(token: &mut OAuthTokenResponse) {
    token.access_token.zeroize();
    if let Some(refresh_token) = &mut token.refresh_token {
        refresh_token.zeroize();
    }
}

pub(super) fn refreshed_scopes(
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

pub(super) fn token_expiry(expires_in: Option<u64>) -> Option<String> {
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

pub(super) fn validate_callback_error(value: &str) -> Result<String> {
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

pub(super) fn validate_callback_error_description(value: &str) -> Result<String> {
    required_value("Plugin OAuth callback error description", value, 1024)
}

pub(super) fn random_urlsafe(bytes: usize) -> Result<String> {
    let mut value = vec![0_u8; bytes];
    SystemRandom::new()
        .fill(value.as_mut_slice())
        .map_err(|_| anyhow::anyhow!("generate Plugin OAuth random value failed"))?;
    Ok(URL_SAFE_NO_PAD.encode(value))
}

pub(super) fn prune_pending(pending: &mut HashMap<String, PendingOAuthTransaction>) {
    let now = Utc::now().timestamp();
    pending.retain(|_, transaction| {
        let active = transaction.expires_at > now;
        if !active {
            transaction.code_verifier.zeroize();
        }
        active
    });
}

pub(super) fn timestamp_rfc3339(timestamp: i64) -> Result<String> {
    DateTime::from_timestamp(timestamp, 0)
        .context("format Plugin OAuth timestamp")
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
}

pub(super) fn token_scope(
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

pub(super) fn validate_connection_expiry(connection: &LocalPluginOAuthConnection) -> Result<()> {
    if let Some(expires_at) = connection.expires_at.as_deref() {
        let expires_at = DateTime::parse_from_rfc3339(expires_at)
            .context("parse Plugin OAuth connection expiry")?;
        if expires_at.timestamp() <= Utc::now().timestamp() {
            bail!("Plugin OAuth access token is expired");
        }
    }
    Ok(())
}

pub(super) fn connection_needs_refresh(connection: &LocalPluginOAuthConnection) -> Result<bool> {
    let Some(expires_at) = connection.expires_at.as_deref() else {
        return Ok(false);
    };
    let expires_at =
        DateTime::parse_from_rfc3339(expires_at).context("parse Plugin OAuth connection expiry")?;
    Ok(expires_at.timestamp() <= Utc::now().timestamp() + OAUTH_REFRESH_EARLY_SECONDS)
}

pub(super) fn connection_snapshot_sha256(connection: &LocalPluginOAuthConnection) -> String {
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

pub(super) fn connection_binding_sha256(connection: &LocalPluginOAuthConnection) -> String {
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

pub(super) fn connection_key_for(connection: &LocalPluginOAuthConnection) -> String {
    connection_storage_key(
        connection.owner_user_id.as_str(),
        connection.device_id.as_str(),
        connection.plugin_id.as_str(),
        connection.component_key.as_str(),
        connection.provider.as_str(),
    )
}

pub(super) fn connection_storage_key(
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

pub(super) fn load_state(path: &Path) -> Result<OAuthConnectionState> {
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

pub(super) fn save_state(path: &Path, state: &OAuthConnectionState) -> Result<()> {
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
