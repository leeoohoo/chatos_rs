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

#[cfg(not(unix))]
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
mod tests {
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use chatos_sandbox_contract::{
        ManagedRequirementsBundleLayer, ManagedRequirementsBundlePayload,
    };

    use super::*;
    use crate::sandbox::managed_requirements_cache::store_test_bundle_with_signer;

    struct TestBundle {
        identity: ManagedRequirementsIdentity,
        client_config: ManagedRequirementsClientConfig,
        keypair: Ed25519KeyPair,
    }

    impl TestBundle {
        fn new() -> Self {
            let service_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            let keypair = Ed25519KeyPair::from_pkcs8(service_pkcs8.as_ref()).unwrap();
            let service_public_key = format!(
                "ed25519:{}",
                URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
            );
            let device_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            let device_keypair = Ed25519KeyPair::from_pkcs8(device_pkcs8.as_ref()).unwrap();
            Self {
                identity: ManagedRequirementsIdentity {
                    cloud_base_url: "https://connector.example.test".to_string(),
                    owner_user_id: "user-1".to_string(),
                    device_id: "device-1".to_string(),
                    device_public_key: format!(
                        "ed25519:{}",
                        URL_SAFE_NO_PAD.encode(device_keypair.public_key().as_ref())
                    ),
                },
                client_config: ManagedRequirementsClientConfig {
                    schema_version: CLIENT_CONFIG_SCHEMA_VERSION,
                    trusted_signing_keys: BTreeMap::from([(
                        "service-key-1".to_string(),
                        service_public_key,
                    )]),
                    fetch_attempts: 3,
                    retry_delay_ms: 50,
                    request_timeout_ms: 1_000,
                    minimum_bundle_issued_at: None,
                },
                keypair,
            }
        }

        fn bundle(&self, now: DateTime<Utc>) -> ManagedRequirementsBundle {
            let requirements_toml = r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
"#;
            let payload = ManagedRequirementsBundlePayload {
                schema_version: MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
                key_id: "service-key-1".to_string(),
                cloud_base_url: self.identity.cloud_base_url.clone(),
                owner_user_id: self.identity.owner_user_id.clone(),
                device_id: self.identity.device_id.clone(),
                device_public_key: self.identity.device_public_key.clone(),
                issued_at: (now - Duration::minutes(1)).to_rfc3339(),
                expires_at: (now + Duration::hours(1)).to_rfc3339(),
                layers: vec![ManagedRequirementsBundleLayer {
                    policy_id: "policy-1".to_string(),
                    policy_version: 1,
                    assignment_id: "assignment-1".to_string(),
                    assignment_scope: "global".to_string(),
                    requirements_toml: requirements_toml.to_string(),
                    requirements_sha256: requirements_digest(requirements_toml.as_bytes()),
                }],
            };
            let signed = managed_requirements_bundle_signature_payload(&payload).unwrap();
            ManagedRequirementsBundle {
                payload,
                signature: URL_SAFE_NO_PAD.encode(self.keypair.sign(signed.as_slice()).as_ref()),
            }
        }

        fn resign(&self, bundle: &mut ManagedRequirementsBundle) {
            let signed = managed_requirements_bundle_signature_payload(&bundle.payload).unwrap();
            bundle.signature =
                URL_SAFE_NO_PAD.encode(self.keypair.sign(signed.as_slice()).as_ref());
        }
    }

    #[test]
    fn valid_service_signed_bundle_is_accepted() {
        let test = TestBundle::new();
        let now = Utc::now();

        let verified = verify_bundle(&test.bundle(now), &test.identity, &test.client_config, now)
            .expect("valid bundle");

        assert_eq!(
            verified
                .document
                .as_ref()
                .and_then(|document| document.default_permissions.as_deref()),
            Some(":read-only")
        );
    }

    #[test]
    fn service_signature_content_expiry_and_identity_are_fail_closed() {
        let test = TestBundle::new();
        let now = Utc::now();

        let mut signature = test.bundle(now);
        signature.signature.push('A');
        assert!(format!(
            "{:#}",
            verify_bundle(&signature, &test.identity, &test.client_config, now).unwrap_err()
        )
        .contains("signature"));

        let mut content = test.bundle(now);
        content.payload.layers[0].requirements_toml =
            "default_permissions = \":workspace\"".to_string();
        test.resign(&mut content);
        assert!(format!(
            "{:#}",
            verify_bundle(&content, &test.identity, &test.client_config, now).unwrap_err()
        )
        .contains("digest"));

        let mut expired = test.bundle(now);
        expired.payload.issued_at = (now - Duration::hours(2)).to_rfc3339();
        expired.payload.expires_at = (now - Duration::hours(1)).to_rfc3339();
        test.resign(&mut expired);
        assert!(format!(
            "{:#}",
            verify_bundle(&expired, &test.identity, &test.client_config, now).unwrap_err()
        )
        .contains("expired"));

        let mut wrong_identity = test.identity.clone();
        wrong_identity.owner_user_id = "user-2".to_string();
        assert!(format!(
            "{:#}",
            verify_bundle(&test.bundle(now), &wrong_identity, &test.client_config, now)
                .unwrap_err()
        )
        .contains("current pairing"));
    }

    #[test]
    fn untrusted_service_key_is_rejected() {
        let test = TestBundle::new();
        let now = Utc::now();
        let mut config = test.client_config.clone();
        config.trusted_signing_keys.clear();

        let error = verify_bundle(&test.bundle(now), &test.identity, &config, now)
            .expect_err("untrusted key must fail");

        assert!(error.to_string().contains("not trusted"));
    }

    #[test]
    fn higher_precedence_bundle_layers_override_lower_layers() {
        let test = TestBundle::new();
        let now = Utc::now();
        let mut bundle = test.bundle(now);
        let requirements_toml = r#"
default_permissions = ":workspace"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#;
        bundle.payload.layers.push(ManagedRequirementsBundleLayer {
            policy_id: "policy-user".to_string(),
            policy_version: 2,
            assignment_id: "assignment-user".to_string(),
            assignment_scope: "user".to_string(),
            requirements_toml: requirements_toml.to_string(),
            requirements_sha256: requirements_digest(requirements_toml.as_bytes()),
        });
        test.resign(&mut bundle);

        let verified = verify_bundle(&bundle, &test.identity, &test.client_config, now).unwrap();

        assert_eq!(
            verified
                .document
                .and_then(|document| document.default_permissions),
            Some(":workspace".to_string())
        );
    }

    #[test]
    fn empty_layers_are_an_explicit_valid_no_requirements_bundle() {
        let test = TestBundle::new();
        let now = Utc::now();
        let mut bundle = test.bundle(now);
        bundle.payload.layers.clear();
        test.resign(&mut bundle);

        let verified = verify_bundle(&bundle, &test.identity, &test.client_config, now).unwrap();

        assert!(verified.document.is_none());
    }

    #[test]
    fn managed_layer_rejects_unrelated_top_level_keys() {
        let test = TestBundle::new();
        let now = Utc::now();
        let mut bundle = test.bundle(now);
        bundle.payload.layers[0].requirements_toml = "model = \"gpt-test\"".to_string();
        bundle.payload.layers[0].requirements_sha256 =
            requirements_digest(bundle.payload.layers[0].requirements_toml.as_bytes());
        test.resign(&mut bundle);

        let error = verify_bundle(&bundle, &test.identity, &test.client_config, now)
            .expect_err("unrelated managed policy keys must fail closed");

        assert!(format!("{error:#}").contains("unsupported managed requirements"));
    }

    #[test]
    fn signing_keys_can_overlap_during_rotation_and_removed_keys_are_rejected() {
        let test = TestBundle::new();
        let now = Utc::now();
        let new_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let new_keypair = Ed25519KeyPair::from_pkcs8(new_pkcs8.as_ref()).unwrap();
        let new_public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(new_keypair.public_key().as_ref())
        );
        let mut config = test.client_config.clone();
        config
            .trusted_signing_keys
            .insert("service-key-2".to_string(), new_public_key);
        let old_bundle = test.bundle(now);
        let mut new_bundle = old_bundle.clone();
        new_bundle.payload.key_id = "service-key-2".to_string();
        let signed = managed_requirements_bundle_signature_payload(&new_bundle.payload).unwrap();
        new_bundle.signature = URL_SAFE_NO_PAD.encode(new_keypair.sign(signed.as_slice()).as_ref());

        assert!(verify_bundle(&old_bundle, &test.identity, &config, now).is_ok());
        assert!(verify_bundle(&new_bundle, &test.identity, &config, now).is_ok());
        config.trusted_signing_keys.remove("service-key-1");
        assert!(verify_bundle(&old_bundle, &test.identity, &config, now).is_err());
        assert!(verify_bundle(&new_bundle, &test.identity, &config, now).is_ok());
    }

    #[test]
    fn trust_root_minimum_issue_time_and_cached_issue_time_prevent_rollback() {
        let test = TestBundle::new();
        let now = Utc::now();
        let mut config = test.client_config.clone();
        config.minimum_bundle_issued_at = Some(now.to_rfc3339());

        let error = verify_bundle(&test.bundle(now), &test.identity, &config, now)
            .expect_err("bundle older than trust-root floor must fail");
        assert!(error.to_string().contains("rollback detected"));
        assert!(ensure_bundle_not_older(now - Duration::seconds(1), now).is_err());
        assert!(ensure_bundle_not_older(now, now).is_ok());
    }

    #[test]
    fn client_config_rejects_unbounded_retry_or_timeout_values() {
        let test = TestBundle::new();
        let mut config = test.client_config;
        config.fetch_attempts = 0;
        assert!(validate_client_config(&config).is_err());
        config.fetch_attempts = 3;
        config.request_timeout_ms = 60_000;
        assert!(validate_client_config(&config).is_err());
    }

    fn test_state_path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "chatos-managed-startup-{label}-{}",
                uuid::Uuid::new_v4()
            ))
            .join("state.json")
    }

    fn paired_state(identity: &ManagedRequirementsIdentity) -> LocalState {
        LocalState {
            paired_cloud_base_url: Some(identity.cloud_base_url.clone()),
            paired_user_id: Some(identity.owner_user_id.clone()),
            device_id: Some(identity.device_id.clone()),
            device_public_key: Some(identity.device_public_key.clone()),
            ..Default::default()
        }
    }

    fn unavailable_connector_config(state_path: PathBuf) -> ClientConfig {
        ClientConfig {
            cloud_base_url: "https://connector.example.test".to_string(),
            access_token: "test-token".to_string(),
            device_name: "test-device".to_string(),
            public_key: None,
            workspace_path: None,
            workspace_alias: None,
            state_path,
        }
    }

    #[tokio::test]
    async fn valid_cache_is_used_without_waiting_for_failed_network_refresh() {
        let test = TestBundle::new();
        let now = Utc::now();
        let state_path = test_state_path("cache-fallback");
        let bundle = test.bundle(now);
        let device_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let device_keypair = Ed25519KeyPair::from_pkcs8(device_pkcs8.as_ref()).unwrap();
        let mut identity = test.identity.clone();
        identity.device_public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(device_keypair.public_key().as_ref())
        );
        let mut bundle = bundle;
        bundle.payload.device_public_key = identity.device_public_key.clone();
        let signed = managed_requirements_bundle_signature_payload(&bundle.payload).unwrap();
        bundle.signature = URL_SAFE_NO_PAD.encode(test.keypair.sign(signed.as_slice()).as_ref());
        store_test_bundle_with_signer(state_path.as_path(), &identity, &bundle, |payload| {
            URL_SAFE_NO_PAD.encode(device_keypair.sign(payload).as_ref())
        })
        .unwrap();
        let state = paired_state(&identity);
        let connector_config = unavailable_connector_config(state_path.clone());
        let http_client = reqwest::Client::new();

        let resolved = resolve_startup_managed_requirements(
            &http_client,
            state_path.as_path(),
            &state,
            Some(&connector_config),
            Some(test.client_config),
        )
        .await
        .expect("valid cache should be used immediately");

        assert_eq!(
            resolved
                .document
                .as_ref()
                .and_then(|document| document.default_permissions.as_deref()),
            Some(":read-only")
        );
        assert!(resolved.background_refresh.is_some());
        let _ = fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[tokio::test]
    async fn expired_trusted_cache_prevents_an_older_fetched_bundle_rollback() {
        let mut test = TestBundle::new();
        test.client_config.fetch_attempts = 1;
        let now = Utc::now();
        let state_path = test_state_path("rollback-cache");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cloud_base_url = format!("http://{}", listener.local_addr().unwrap());
        test.identity.cloud_base_url = cloud_base_url.clone();
        let device_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let device_keypair = Ed25519KeyPair::from_pkcs8(device_pkcs8.as_ref()).unwrap();
        test.identity.device_public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(device_keypair.public_key().as_ref())
        );

        let mut cached = test.bundle(now);
        cached.payload.issued_at = (now - Duration::minutes(30)).to_rfc3339();
        cached.payload.expires_at = (now - Duration::minutes(1)).to_rfc3339();
        test.resign(&mut cached);
        store_test_bundle_with_signer(state_path.as_path(), &test.identity, &cached, |payload| {
            URL_SAFE_NO_PAD.encode(device_keypair.sign(payload).as_ref())
        })
        .unwrap();

        let mut fetched = test.bundle(now);
        fetched.payload.issued_at = (now - Duration::hours(1)).to_rfc3339();
        fetched.payload.expires_at = (now + Duration::hours(1)).to_rfc3339();
        test.resign(&mut fetched);
        let app = axum::Router::new().route(
            "/api/local-connectors/devices/{id}/managed-requirements",
            axum::routing::get(move || {
                let fetched = fetched.clone();
                async move { axum::Json(fetched) }
            }),
        );
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let state = paired_state(&test.identity);
        let mut connector_config = unavailable_connector_config(state_path.clone());
        connector_config.cloud_base_url = cloud_base_url;
        let http_client = reqwest::Client::new();

        let error = resolve_startup_managed_requirements(
            &http_client,
            state_path.as_path(),
            &state,
            Some(&connector_config),
            Some(test.client_config),
        )
        .await
        .err()
        .expect("an older fetched bundle must not replace a trusted expired cache");

        assert!(format!("{error:#}").contains("rollback detected"));
        assert_eq!(
            load_cached_bundle(state_path.as_path(), &test.identity)
                .unwrap()
                .unwrap()
                .payload
                .issued_at,
            cached.payload.issued_at
        );
        server.abort();
        let _ = fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[tokio::test]
    async fn failed_fetch_without_valid_cache_is_fail_closed() {
        let mut test = TestBundle::new();
        test.client_config.fetch_attempts = 1;
        test.client_config.request_timeout_ms = 1_000;
        let state_path = test_state_path("no-cache");
        let state = paired_state(&test.identity);
        let mut connector_config = unavailable_connector_config(state_path.clone());
        connector_config.cloud_base_url = "http://127.0.0.1:9".to_string();
        let mut state = state;
        state.paired_cloud_base_url = Some(connector_config.cloud_base_url.clone());
        let http_client = reqwest::Client::new();

        let error = resolve_startup_managed_requirements(
            &http_client,
            state_path.as_path(),
            &state,
            Some(&connector_config),
            Some(test.client_config),
        )
        .await
        .err()
        .expect("missing cache and failed fetch must block startup");

        assert!(format!("{error:#}").contains("no valid managed requirements cache"));
        let _ = fs::remove_dir_all(state_path.parent().unwrap());
    }
}
