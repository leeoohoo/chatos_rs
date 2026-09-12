// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Immutable Plugin Release evidence consumed by the native Local Agent Host.
//!
//! The Host verifies the exact signed normalized-manifest bytes. Mutable
//! catalog, preference, installation workflow, service client, and cache DTOs
//! deliberately do not cross this boundary.

use std::collections::{BTreeMap, BTreeSet};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::DateTime;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PLUGIN_MANIFEST_SCHEMA_VERSION: u32 = 3;
pub const PLUGIN_SIGNATURE_ALGORITHM_ED25519: &str = "ed25519";
pub const PLUGIN_SIGNING_KEY_USAGE_RELEASE: &str = "release";
pub const PLUGIN_RELEASE_SIGNATURE_PURPOSE_V1: &str = "chatos.plugin.release.v1";
pub const MAX_SIGNED_MANIFEST_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedPluginSigningKey {
    pub key_id: String,
    pub publisher_id: String,
    pub algorithm: String,
    pub public_key_base64: String,
    #[serde(default)]
    pub usages: Vec<String>,
    pub valid_from: String,
    #[serde(default)]
    pub valid_until: Option<String>,
    #[serde(default)]
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginReleaseSignature {
    pub key_id: String,
    pub publisher_id: String,
    pub marketplace_id: String,
    pub algorithm: String,
    pub signature_base64: String,
    pub signed_at: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginReleaseVerificationContext<'a> {
    pub plugin_id: &'a str,
    pub version: &'a str,
    pub marketplace_id: &'a str,
    pub publisher_id: &'a str,
    pub artifact_sha256: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedPluginManifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub mcp_servers: Vec<SignedPluginMcpServer>,
    #[serde(default)]
    pub dependencies: SignedPluginDependencies,
    #[serde(default)]
    pub permissions: Vec<SignedPluginPermission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum SignedPluginMcpServer {
    Stdio {
        component_key: String,
        bin: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        requires_exclusive_execution: bool,
    },
    Http {
        component_key: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        oauth_resource: Option<String>,
        #[serde(default)]
        connect_timeout_ms: Option<u64>,
        #[serde(default)]
        requires_exclusive_execution: bool,
    },
}

impl SignedPluginMcpServer {
    pub fn component_key(&self) -> &str {
        match self {
            Self::Stdio { component_key, .. } | Self::Http { component_key, .. } => component_key,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedPluginDependencies {
    #[serde(default)]
    pub supported_platforms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SignedPluginPermission {
    pub permission: String,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub components: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PluginCapabilityVerificationError {
    #[error("signed Plugin manifest base64 is invalid")]
    InvalidManifestBase64,
    #[error("signed Plugin manifest exceeds {maximum} bytes")]
    ManifestTooLarge { maximum: usize },
    #[error("signed Plugin manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("Plugin Release {field} must be a lower-case SHA-256 digest")]
    InvalidSha256 { field: &'static str },
    #[error("Plugin Release manifest digest does not match its signature")]
    ManifestDigestMismatch,
    #[error("Plugin Release signature identity mismatch: {field}")]
    IdentityMismatch { field: &'static str },
    #[error("unsupported Plugin Release signature algorithm")]
    UnsupportedAlgorithm,
    #[error("Plugin Release signing key is not authorized for Release signatures")]
    UnauthorizedSigningKey,
    #[error("Plugin Release signing key has been revoked")]
    RevokedSigningKey,
    #[error("Plugin Release signature timestamp is invalid: {field}")]
    InvalidTimestamp { field: &'static str },
    #[error("Plugin Release signature is outside the signing key validity window")]
    OutsideSigningKeyValidity,
    #[error("Plugin Release signing key or signature encoding is invalid: {field}")]
    InvalidEncoding { field: &'static str },
    #[error("Plugin Release signature verification failed")]
    InvalidSignature,
}

pub fn verify_signed_plugin_manifest(
    context: PluginReleaseVerificationContext<'_>,
    manifest_payload_base64: &str,
    signature: &PluginReleaseSignature,
    trusted_key: &TrustedPluginSigningKey,
) -> Result<SignedPluginManifest, PluginCapabilityVerificationError> {
    validate_context(context)?;
    validate_signature_identity(context, signature, trusted_key)?;
    let maximum_encoded = MAX_SIGNED_MANIFEST_BYTES.saturating_mul(4) / 3 + 4;
    if manifest_payload_base64.len() > maximum_encoded {
        return Err(PluginCapabilityVerificationError::ManifestTooLarge {
            maximum: MAX_SIGNED_MANIFEST_BYTES,
        });
    }
    let manifest_bytes = STANDARD
        .decode(manifest_payload_base64.as_bytes())
        .map_err(|_| PluginCapabilityVerificationError::InvalidManifestBase64)?;
    if manifest_bytes.len() > MAX_SIGNED_MANIFEST_BYTES {
        return Err(PluginCapabilityVerificationError::ManifestTooLarge {
            maximum: MAX_SIGNED_MANIFEST_BYTES,
        });
    }
    let digest = hex::encode(Sha256::digest(manifest_bytes.as_slice()));
    if digest != signature.manifest_sha256 {
        return Err(PluginCapabilityVerificationError::ManifestDigestMismatch);
    }
    validate_signing_window(signature, trusted_key)?;
    let public_key = decode_exact(
        "trusted_key.public_key_base64",
        trusted_key.public_key_base64.as_str(),
        32,
    )?;
    let signature_bytes = decode_exact(
        "signature.signature_base64",
        signature.signature_base64.as_str(),
        64,
    )?;
    let signing_payload = plugin_release_signing_payload(context, signature)?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(signing_payload.as_slice(), signature_bytes.as_slice())
        .map_err(|_| PluginCapabilityVerificationError::InvalidSignature)?;
    let manifest: SignedPluginManifest = serde_json::from_slice(manifest_bytes.as_slice())
        .map_err(|error| PluginCapabilityVerificationError::InvalidManifest(error.to_string()))?;
    validate_manifest(context, &manifest)?;
    Ok(manifest)
}

pub fn plugin_release_signing_payload(
    context: PluginReleaseVerificationContext<'_>,
    signature: &PluginReleaseSignature,
) -> Result<Vec<u8>, PluginCapabilityVerificationError> {
    validate_sha256("manifest_sha256", signature.manifest_sha256.as_str())?;
    validate_sha256("artifact_sha256", context.artifact_sha256)?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope<'a> {
        schema_version: u32,
        purpose: &'static str,
        plugin_id: &'a str,
        version: &'a str,
        marketplace_id: &'a str,
        publisher_id: &'a str,
        key_id: &'a str,
        algorithm: &'a str,
        signed_at: &'a str,
        manifest_sha256: &'a str,
        artifact_sha256: &'a str,
    }
    serde_json::to_vec(&Envelope {
        schema_version: 1,
        purpose: PLUGIN_RELEASE_SIGNATURE_PURPOSE_V1,
        plugin_id: context.plugin_id,
        version: context.version,
        marketplace_id: context.marketplace_id,
        publisher_id: context.publisher_id,
        key_id: signature.key_id.as_str(),
        algorithm: signature.algorithm.as_str(),
        signed_at: signature.signed_at.as_str(),
        manifest_sha256: signature.manifest_sha256.as_str(),
        artifact_sha256: context.artifact_sha256,
    })
    .map_err(|error| PluginCapabilityVerificationError::InvalidManifest(error.to_string()))
}

fn validate_context(
    context: PluginReleaseVerificationContext<'_>,
) -> Result<(), PluginCapabilityVerificationError> {
    for (field, value) in [
        ("plugin_id", context.plugin_id),
        ("version", context.version),
        ("marketplace_id", context.marketplace_id),
        ("publisher_id", context.publisher_id),
    ] {
        if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
            return Err(PluginCapabilityVerificationError::IdentityMismatch { field });
        }
    }
    validate_sha256("artifact_sha256", context.artifact_sha256)
}

fn validate_signature_identity(
    context: PluginReleaseVerificationContext<'_>,
    signature: &PluginReleaseSignature,
    trusted_key: &TrustedPluginSigningKey,
) -> Result<(), PluginCapabilityVerificationError> {
    if signature.algorithm != PLUGIN_SIGNATURE_ALGORITHM_ED25519
        || trusted_key.algorithm != PLUGIN_SIGNATURE_ALGORITHM_ED25519
    {
        return Err(PluginCapabilityVerificationError::UnsupportedAlgorithm);
    }
    for (field, matches) in [
        (
            "signature.marketplace_id",
            signature.marketplace_id == context.marketplace_id,
        ),
        (
            "signature.publisher_id",
            signature.publisher_id == context.publisher_id,
        ),
        ("signature.key_id", signature.key_id == trusted_key.key_id),
        (
            "trusted_key.publisher_id",
            trusted_key.publisher_id == context.publisher_id,
        ),
    ] {
        if !matches {
            return Err(PluginCapabilityVerificationError::IdentityMismatch { field });
        }
    }
    if !trusted_key
        .usages
        .iter()
        .any(|usage| usage == PLUGIN_SIGNING_KEY_USAGE_RELEASE)
    {
        return Err(PluginCapabilityVerificationError::UnauthorizedSigningKey);
    }
    Ok(())
}

fn validate_signing_window(
    signature: &PluginReleaseSignature,
    trusted_key: &TrustedPluginSigningKey,
) -> Result<(), PluginCapabilityVerificationError> {
    let signed_at = parse_timestamp("signature.signed_at", signature.signed_at.as_str())?;
    let valid_from = parse_timestamp("trusted_key.valid_from", trusted_key.valid_from.as_str())?;
    let valid_until = trusted_key
        .valid_until
        .as_deref()
        .map(|value| parse_timestamp("trusted_key.valid_until", value))
        .transpose()?;
    if let Some(revoked_at) = trusted_key.revoked_at.as_deref() {
        parse_timestamp("trusted_key.revoked_at", revoked_at)?;
        return Err(PluginCapabilityVerificationError::RevokedSigningKey);
    }
    if signed_at < valid_from || valid_until.is_some_and(|until| signed_at > until) {
        return Err(PluginCapabilityVerificationError::OutsideSigningKeyValidity);
    }
    Ok(())
}

fn validate_manifest(
    context: PluginReleaseVerificationContext<'_>,
    manifest: &SignedPluginManifest,
) -> Result<(), PluginCapabilityVerificationError> {
    if manifest.schema_version != PLUGIN_MANIFEST_SCHEMA_VERSION
        || manifest.name.is_empty()
        || manifest.name.trim() != manifest.name
        || manifest.version != context.version
    {
        return Err(PluginCapabilityVerificationError::InvalidManifest(
            "schema, name, or version does not match the signed Release".to_string(),
        ));
    }
    let mut components = BTreeSet::new();
    for server in &manifest.mcp_servers {
        let key = server.component_key();
        if key.is_empty() || key.trim() != key || !components.insert(key) {
            return Err(PluginCapabilityVerificationError::InvalidManifest(
                "MCP component keys are invalid or duplicated".to_string(),
            ));
        }
        if let SignedPluginMcpServer::Stdio { bin, args, env, .. } = server {
            if bin.is_empty()
                || bin.trim() != bin
                || args.iter().any(|value| value.contains('\0'))
                || env.iter().any(|(name, value)| {
                    name.is_empty() || name.trim() != name || value.contains('\0')
                })
            {
                return Err(PluginCapabilityVerificationError::InvalidManifest(
                    "stdio MCP declaration is invalid".to_string(),
                ));
            }
        }
    }
    for permission in &manifest.permissions {
        if permission.permission.is_empty()
            || permission.permission.trim() != permission.permission
            || permission
                .components
                .iter()
                .any(|key| !components.contains(key.as_str()))
        {
            return Err(PluginCapabilityVerificationError::InvalidManifest(
                "Plugin permission declaration is invalid".to_string(),
            ));
        }
    }
    Ok(())
}

fn parse_timestamp(
    field: &'static str,
    value: &str,
) -> Result<DateTime<chrono::FixedOffset>, PluginCapabilityVerificationError> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|_| PluginCapabilityVerificationError::InvalidTimestamp { field })
}

fn validate_sha256(
    field: &'static str,
    value: &str,
) -> Result<(), PluginCapabilityVerificationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(PluginCapabilityVerificationError::InvalidSha256 { field })
    }
}

fn decode_exact(
    field: &'static str,
    value: &str,
    expected_length: usize,
) -> Result<Vec<u8>, PluginCapabilityVerificationError> {
    let decoded = STANDARD
        .decode(value.as_bytes())
        .map_err(|_| PluginCapabilityVerificationError::InvalidEncoding { field })?;
    if decoded.len() == expected_length {
        Ok(decoded)
    } else {
        Err(PluginCapabilityVerificationError::InvalidEncoding { field })
    }
}

const fn default_true() -> bool {
    true
}
