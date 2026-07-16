// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chatos_sandbox_contract::{
    managed_requirements_bundle_signature_payload, merge_codex_permission_profile_document_layers,
    parse_managed_requirements_toml, CodexPermissionProfileDocument, ManagedRequirementsBundle,
    MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
};
use chrono::{DateTime, Duration, Utc};
use futures_util::StreamExt as _;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::{api_url, ClientConfig};
use crate::tracing_stdout;
use crate::LocalState;

use super::managed_requirements_cache::{
    canonical_cloud_base_url, load_cached_bundle, store_verified_bundle,
    ManagedRequirementsIdentity,
};

const CLIENT_CONFIG_SCHEMA_VERSION: u32 = 1;
const MAX_CLIENT_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_BUNDLE_BYTES: usize = 1024 * 1024 + 64 * 1024;
const MAX_REQUIREMENTS_BYTES: usize = 1024 * 1024;
const MAX_REQUIREMENTS_LAYERS: usize = 64;
const MAX_ISSUED_AT_CLOCK_SKEW_MINUTES: i64 = 5;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagedRequirementsClientConfig {
    schema_version: u32,
    trusted_signing_keys: BTreeMap<String, String>,
    #[serde(default = "default_fetch_attempts")]
    fetch_attempts: u8,
    #[serde(default = "default_retry_delay_ms")]
    retry_delay_ms: u64,
    #[serde(default = "default_request_timeout_ms")]
    request_timeout_ms: u64,
    #[serde(default)]
    minimum_bundle_issued_at: Option<String>,
}

pub(crate) struct StartupManagedRequirements {
    pub(crate) document: Option<CodexPermissionProfileDocument>,
    pub(crate) background_refresh: Option<ManagedRequirementsRefresh>,
}

pub(crate) struct ManagedRequirementsRefresh {
    client_config: ManagedRequirementsClientConfig,
    connector_config: ClientConfig,
    identity: ManagedRequirementsIdentity,
    state_path: PathBuf,
    minimum_issued_at: DateTime<Utc>,
}

#[derive(Debug)]
struct VerifiedManagedRequirementsBundle {
    document: Option<CodexPermissionProfileDocument>,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl StartupManagedRequirements {
    fn none() -> Self {
        Self {
            document: None,
            background_refresh: None,
        }
    }
}

impl ManagedRequirementsRefresh {
    pub(crate) fn spawn(self, http_client: reqwest::Client) {
        tokio::spawn(async move {
            if let Err(err) = self.refresh(&http_client).await {
                tracing_stdout(
                    format!(
                        "background managed requirements refresh failed; current process policy remains unchanged: {err:#}"
                    )
                    .as_str(),
                );
            }
        });
    }

    async fn refresh(&self, http_client: &reqwest::Client) -> Result<()> {
        let bundle = fetch_bundle_with_retries(
            http_client,
            &self.connector_config,
            &self.identity,
            &self.client_config,
        )
        .await?;
        let verified = verify_bundle(&bundle, &self.identity, &self.client_config, Utc::now())?;
        ensure_bundle_not_older(verified.issued_at, self.minimum_issued_at)?;
        store_verified_bundle(self.state_path.as_path(), &self.identity, &bundle)
            .context("update managed requirements cache")
    }
}

pub(crate) fn load_system_client_config() -> Result<Option<ManagedRequirementsClientConfig>> {
    let Some(path) = default_client_config_path() else {
        return Ok(None);
    };
    load_client_config(path.as_path(), true)
}

pub(crate) async fn resolve_startup_managed_requirements(
    http_client: &reqwest::Client,
    state_path: &Path,
    state: &LocalState,
    connector_config: Option<&ClientConfig>,
    client_config: Option<ManagedRequirementsClientConfig>,
) -> Result<StartupManagedRequirements> {
    let Some(client_config) = client_config else {
        return Ok(StartupManagedRequirements::none());
    };
    let Some(connector_config) = connector_config else {
        return Ok(StartupManagedRequirements::none());
    };
    let identity = ManagedRequirementsIdentity::from_state(state)?.ok_or_else(|| {
        anyhow!("managed requirements are enabled but pairing identity is incomplete")
    })?;
    let configured_cloud = canonical_cloud_base_url(connector_config.cloud_base_url.as_str())?;
    if configured_cloud != identity.cloud_base_url {
        return Err(anyhow!(
            "managed requirements pairing cloud does not match the active connector configuration"
        ));
    }

    let mut cache_error = None;
    let mut cached_issued_at = None;
    match load_cached_bundle(state_path, &identity) {
        Ok(Some(bundle)) => match verify_bundle_authenticity(&bundle, &identity, &client_config) {
            Ok(verified) => {
                cached_issued_at = Some(verified.issued_at);
                match ensure_bundle_current(&verified, Utc::now()) {
                    Ok(()) => {
                        return Ok(StartupManagedRequirements {
                            document: verified.document,
                            background_refresh: Some(ManagedRequirementsRefresh {
                                client_config,
                                connector_config: connector_config.clone(),
                                identity,
                                state_path: state_path.to_path_buf(),
                                minimum_issued_at: verified.issued_at,
                            }),
                        })
                    }
                    Err(err) => cache_error = Some(err.context("cached bundle is invalid")),
                }
            }
            Err(err) => cache_error = Some(err.context("cached bundle is invalid")),
        },
        Ok(None) => {}
        Err(err) => cache_error = Some(err),
    }

    let fetched =
        fetch_bundle_with_retries(http_client, connector_config, &identity, &client_config).await;
    let bundle = match fetched {
        Ok(bundle) => bundle,
        Err(fetch_error) => {
            return Err(match cache_error {
                Some(cache_error) => fetch_error.context(format!(
                    "no valid managed requirements cache was available: {cache_error:#}"
                )),
                None => fetch_error.context("no valid managed requirements cache was available"),
            })
        }
    };
    let verified = verify_bundle(&bundle, &identity, &client_config, Utc::now())
        .context("verify fetched managed requirements bundle")?;
    if let Some(cached_issued_at) = cached_issued_at {
        ensure_bundle_not_older(verified.issued_at, cached_issued_at)?;
    }
    store_verified_bundle(state_path, &identity, &bundle)
        .context("write fetched managed requirements cache")?;
    Ok(StartupManagedRequirements {
        document: verified.document,
        background_refresh: None,
    })
}

async fn fetch_bundle_with_retries(
    http_client: &reqwest::Client,
    connector_config: &ClientConfig,
    identity: &ManagedRequirementsIdentity,
    client_config: &ManagedRequirementsClientConfig,
) -> Result<ManagedRequirementsBundle> {
    let mut last_error = None;
    for attempt in 0..client_config.fetch_attempts {
        match fetch_bundle_once(
            http_client,
            connector_config,
            identity,
            client_config.request_timeout_ms,
        )
        .await
        {
            Ok(bundle) => return Ok(bundle),
            Err(err) => last_error = Some(err),
        }
        if attempt + 1 < client_config.fetch_attempts {
            let multiplier = u64::from(attempt) + 1;
            tokio::time::sleep(StdDuration::from_millis(
                client_config.retry_delay_ms.saturating_mul(multiplier),
            ))
            .await;
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("managed requirements fetch did not run")))
        .context("fetch managed requirements bundle with retries")
}

async fn fetch_bundle_once(
    http_client: &reqwest::Client,
    connector_config: &ClientConfig,
    identity: &ManagedRequirementsIdentity,
    request_timeout_ms: u64,
) -> Result<ManagedRequirementsBundle> {
    let endpoint = api_url(
        connector_config.cloud_base_url.as_str(),
        format!(
            "/api/local-connectors/devices/{}/managed-requirements",
            urlencoding::encode(identity.device_id.as_str())
        )
        .as_str(),
    );
    let response = http_client
        .get(endpoint.as_str())
        .bearer_auth(connector_config.access_token.as_str())
        .timeout(StdDuration::from_millis(request_timeout_ms))
        .send()
        .await
        .context("request managed requirements bundle")?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "managed requirements service returned status {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BUNDLE_BYTES as u64)
    {
        return Err(anyhow!(
            "managed requirements response exceeds the size limit"
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read managed requirements response")?;
        if body.len().saturating_add(chunk.len()) > MAX_BUNDLE_BYTES {
            return Err(anyhow!(
                "managed requirements response exceeds the size limit"
            ));
        }
        body.extend_from_slice(chunk.as_ref());
    }
    serde_json::from_slice(body.as_slice()).context("parse managed requirements response")
}

fn verify_bundle(
    bundle: &ManagedRequirementsBundle,
    identity: &ManagedRequirementsIdentity,
    client_config: &ManagedRequirementsClientConfig,
    now: DateTime<Utc>,
) -> Result<VerifiedManagedRequirementsBundle> {
    let verified = verify_bundle_authenticity(bundle, identity, client_config)?;
    ensure_bundle_current(&verified, now)?;
    Ok(verified)
}

fn verify_bundle_authenticity(
    bundle: &ManagedRequirementsBundle,
    identity: &ManagedRequirementsIdentity,
    client_config: &ManagedRequirementsClientConfig,
) -> Result<VerifiedManagedRequirementsBundle> {
    let payload = &bundle.payload;
    if payload.schema_version != MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported managed requirements bundle schema version {}",
            payload.schema_version
        ));
    }
    let signing_key = client_config
        .trusted_signing_keys
        .get(payload.key_id.as_str())
        .ok_or_else(|| anyhow!("managed requirements bundle signing key is not trusted"))?;
    ensure_identity_field(
        "cloud base URL",
        canonical_cloud_base_url(payload.cloud_base_url.as_str())?.as_str(),
        identity.cloud_base_url.as_str(),
    )?;
    ensure_identity_field(
        "owner user id",
        payload.owner_user_id.as_str(),
        identity.owner_user_id.as_str(),
    )?;
    ensure_identity_field(
        "device id",
        payload.device_id.as_str(),
        identity.device_id.as_str(),
    )?;
    ensure_identity_field(
        "device public key",
        payload.device_public_key.as_str(),
        identity.device_public_key.as_str(),
    )?;
    let issued_at = parse_timestamp("issued_at", payload.issued_at.as_str())?;
    let expires_at = parse_timestamp("expires_at", payload.expires_at.as_str())?;
    if expires_at <= issued_at {
        return Err(anyhow!(
            "managed requirements bundle expires_at must be later than issued_at"
        ));
    }
    if let Some(minimum_issued_at) = client_config.minimum_bundle_issued_at.as_deref() {
        let minimum_issued_at = parse_timestamp("minimum_bundle_issued_at", minimum_issued_at)?;
        ensure_bundle_not_older(issued_at, minimum_issued_at)?;
    }
    let signed_payload = managed_requirements_bundle_signature_payload(payload)
        .context("serialize managed requirements bundle")?;
    verify_service_signature(
        signing_key,
        signed_payload.as_slice(),
        bundle.signature.as_str(),
    )?;
    if payload.layers.len() > MAX_REQUIREMENTS_LAYERS {
        return Err(anyhow!(
            "managed requirements bundle contains too many policy layers"
        ));
    }
    let mut total_requirements_bytes = 0_usize;
    let mut assignment_ids = BTreeSet::new();
    let mut document: Option<CodexPermissionProfileDocument> = None;
    for layer in &payload.layers {
        validate_bundle_layer_metadata(layer.policy_id.as_str(), "policy_id")?;
        validate_bundle_layer_metadata(layer.assignment_id.as_str(), "assignment_id")?;
        validate_bundle_layer_metadata(layer.assignment_scope.as_str(), "assignment_scope")?;
        if layer.policy_version < 1 {
            return Err(anyhow!(
                "managed requirements bundle policy_version must be positive"
            ));
        }
        if !assignment_ids.insert(layer.assignment_id.as_str()) {
            return Err(anyhow!(
                "managed requirements bundle contains duplicate assignment ids"
            ));
        }
        total_requirements_bytes = total_requirements_bytes
            .checked_add(layer.requirements_toml.len())
            .ok_or_else(|| anyhow!("managed requirements bundle size overflow"))?;
        if total_requirements_bytes > MAX_REQUIREMENTS_BYTES {
            return Err(anyhow!(
                "managed requirements bundle TOML exceeds the 1 MiB aggregate limit"
            ));
        }
        if layer.requirements_sha256 != requirements_digest(layer.requirements_toml.as_bytes()) {
            return Err(anyhow!(
                "managed requirements bundle content digest does not match"
            ));
        }
        let layer_document = parse_managed_requirements_toml(layer.requirements_toml.as_str())
            .map_err(anyhow::Error::msg)
            .context("parse managed requirements bundle TOML")?;
        document = Some(match document {
            Some(lower) => merge_codex_permission_profile_document_layers(lower, layer_document),
            None => layer_document,
        });
    }
    Ok(VerifiedManagedRequirementsBundle {
        document,
        issued_at,
        expires_at,
    })
}

fn ensure_bundle_current(
    bundle: &VerifiedManagedRequirementsBundle,
    now: DateTime<Utc>,
) -> Result<()> {
    if bundle.issued_at > now + Duration::minutes(MAX_ISSUED_AT_CLOCK_SKEW_MINUTES) {
        return Err(anyhow!(
            "managed requirements bundle issued_at is too far in the future"
        ));
    }
    if bundle.expires_at <= now {
        return Err(anyhow!("managed requirements bundle has expired"));
    }
    Ok(())
}

fn ensure_bundle_not_older(candidate: DateTime<Utc>, minimum: DateTime<Utc>) -> Result<()> {
    if candidate < minimum {
        Err(anyhow!(
            "managed requirements bundle rollback detected: issued_at is older than the trusted minimum"
        ))
    } else {
        Ok(())
    }
}

fn validate_bundle_layer_metadata(value: &str, label: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        Err(anyhow!(
            "managed requirements bundle layer {label} is invalid"
        ))
    } else {
        Ok(())
    }
}

fn load_client_config(
    path: &Path,
    secure_system_file: bool,
) -> Result<Option<ManagedRequirementsClientConfig>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| {
                format!("read managed requirements client config {}", path.display())
            })
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!(
            "managed requirements client config {} must be a regular non-symlink file",
            path.display()
        ));
    }
    if secure_system_file {
        validate_secure_system_config(path, &metadata)?;
    }
    if metadata.len() > MAX_CLIENT_CONFIG_BYTES {
        return Err(anyhow!(
            "managed requirements client config {} exceeds the size limit",
            path.display()
        ));
    }
    let bytes = fs::read(path)
        .with_context(|| format!("read managed requirements client config {}", path.display()))?;
    let config = serde_json::from_slice::<ManagedRequirementsClientConfig>(bytes.as_slice())
        .with_context(|| {
            format!(
                "parse managed requirements client config {}",
                path.display()
            )
        })?;
    validate_client_config(&config)?;
    Ok(Some(config))
}

fn validate_client_config(config: &ManagedRequirementsClientConfig) -> Result<()> {
    if config.schema_version != CLIENT_CONFIG_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported managed requirements client config schema version {}",
            config.schema_version
        ));
    }
    if config.trusted_signing_keys.is_empty() {
        return Err(anyhow!(
            "managed requirements client config must contain at least one trusted signing key"
        ));
    }
    for (key_id, public_key) in &config.trusted_signing_keys {
        if key_id.trim().is_empty() {
            return Err(anyhow!(
                "managed requirements trusted signing key id must not be empty"
            ));
        }
        ed25519_public_key_bytes(public_key)?;
    }
    if !(1..=5).contains(&config.fetch_attempts) {
        return Err(anyhow!(
            "managed requirements fetch_attempts must be between 1 and 5"
        ));
    }
    if !(50..=5_000).contains(&config.retry_delay_ms) {
        return Err(anyhow!(
            "managed requirements retry_delay_ms must be between 50 and 5000"
        ));
    }
    if !(1_000..=30_000).contains(&config.request_timeout_ms) {
        return Err(anyhow!(
            "managed requirements request_timeout_ms must be between 1000 and 30000"
        ));
    }
    if let Some(minimum_issued_at) = config.minimum_bundle_issued_at.as_deref() {
        parse_timestamp("minimum_bundle_issued_at", minimum_issued_at)?;
    }
    Ok(())
}

fn verify_service_signature(public_key: &str, payload: &[u8], signature: &str) -> Result<()> {
    let public_key = ed25519_public_key_bytes(public_key)?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature.trim().as_bytes())
        .context("decode managed requirements service signature")?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(payload, signature.as_slice())
        .map_err(|_| anyhow!("managed requirements service signature verification failed"))
}

fn ed25519_public_key_bytes(value: &str) -> Result<Vec<u8>> {
    let encoded = value
        .trim()
        .strip_prefix("ed25519:")
        .ok_or_else(|| anyhow!("managed requirements signing key must be an Ed25519 public key"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .context("decode managed requirements Ed25519 public key")?;
    if bytes.len() != 32 {
        return Err(anyhow!(
            "managed requirements Ed25519 public key must be 32 bytes"
        ));
    }
    Ok(bytes)
}

fn ensure_identity_field(label: &str, actual: &str, expected: &str) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(anyhow!(
            "managed requirements bundle {label} does not match the current pairing"
        ))
    }
}

fn parse_timestamp(label: &str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .with_context(|| format!("managed requirements bundle {label} is invalid"))
}

fn requirements_digest(value: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value)))
}

#[cfg(not(windows))]
fn default_client_config_path() -> Option<PathBuf> {
    Some(PathBuf::from(
        "/etc/chatos/managed-requirements-client.json",
    ))
}

#[cfg(windows)]
fn default_client_config_path() -> Option<PathBuf> {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|path| {
            path.join("ChatOS")
                .join("LocalConnector")
                .join("managed-requirements-client.json")
        })
}

#[cfg(unix)]
fn validate_secure_system_config(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if metadata.uid() != 0 {
        return Err(anyhow!(
            "managed requirements client config {} must be owned by root",
            path.display()
        ));
    }
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(anyhow!(
            "managed requirements client config {} must not be group- or world-writable",
            path.display()
        ));
    }
    if metadata.nlink() != 1 {
        return Err(anyhow!(
            "managed requirements client config {} must not have hard links",
            path.display()
        ));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("/"));
    let parent_metadata = fs::symlink_metadata(parent).with_context(|| {
        format!(
            "read managed requirements client config directory {}",
            parent.display()
        )
    })?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(anyhow!(
            "managed requirements client config directory {} must be a non-symlink directory",
            parent.display()
        ));
    }
    if parent_metadata.uid() != 0 || parent_metadata.permissions().mode() & 0o022 != 0 {
        return Err(anyhow!(
            "managed requirements client config directory {} must be root-owned and not group- or world-writable",
            parent.display()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_secure_system_config(path: &Path, _metadata: &fs::Metadata) -> Result<()> {
    crate::sandbox::windows_security::validate_windows_secure_system_path(
        path,
        "managed requirements client config",
    )
}

#[cfg(all(not(unix), not(windows)))]
fn validate_secure_system_config(path: &Path, _metadata: &fs::Metadata) -> Result<()> {
    Err(anyhow!(
        "managed requirements client config {} cannot be trusted until platform ACL validation is available",
        path.display()
    ))
}

const fn default_fetch_attempts() -> u8 {
    3
}

const fn default_retry_delay_ms() -> u64 {
    250
}

const fn default_request_timeout_ms() -> u64 {
    10_000
}

#[cfg(test)]
mod tests;
