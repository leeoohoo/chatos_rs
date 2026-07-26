// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::DateTime;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::plugin_manifest::PluginManifest;

mod catalog;

pub use catalog::*;

pub const PLUGIN_SIGNATURE_ALGORITHM_ED25519: &str = "ed25519";
pub const PLUGIN_RELEASE_SIGNATURE_PURPOSE_V1: &str = "chatos.plugin.release.v1";
pub const PLUGIN_SIGNING_KEY_USAGE_CATALOG: &str = "catalog";
pub const PLUGIN_SIGNING_KEY_USAGE_RELEASE: &str = "release";

pub fn normalized_plugin_manifest_sha256(
    manifest: &PluginManifest,
) -> Result<String, serde_json::Error> {
    serde_json::to_vec(manifest).map(|canonical| hex::encode(Sha256::digest(canonical)))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigningKeyRef {
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginReleaseSigningEnvelope<'a> {
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

#[derive(Debug, Error)]
pub enum PluginSignatureVerificationError {
    #[error("unsupported Plugin signature algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("Plugin release signature {field} does not match the trusted release identity")]
    IdentityMismatch { field: &'static str },
    #[error("Plugin release {field} must be a lower-case SHA-256 digest")]
    InvalidSha256 { field: &'static str },
    #[error("Plugin release signature manifest hash does not match the normalized manifest")]
    ManifestHashMismatch,
    #[error("Plugin Catalog signature hash does not match the normalized Catalog document")]
    CatalogHashMismatch,
    #[error("invalid Plugin Catalog document: {message}")]
    InvalidCatalogDocument { message: String },
    #[error("Plugin release signing key has been revoked")]
    KeyRevoked,
    #[error("invalid RFC3339 timestamp in {field}")]
    InvalidTimestamp { field: &'static str },
    #[error("Plugin release signature is outside the signing key validity window")]
    OutsideKeyValidity,
    #[error("invalid base64 in {field}")]
    InvalidBase64 { field: &'static str },
    #[error("{field} decoded to {actual} bytes; expected {expected}")]
    InvalidLength {
        field: &'static str,
        actual: usize,
        expected: usize,
    },
    #[error("serialize Plugin release signing payload failed: {0}")]
    SerializePayload(#[source] serde_json::Error),
    #[error("Plugin release Ed25519 signature verification failed")]
    InvalidSignature,
}

pub fn plugin_release_signing_payload(
    context: PluginReleaseVerificationContext<'_>,
    signature: &PluginReleaseSignature,
) -> Result<Vec<u8>, PluginSignatureVerificationError> {
    validate_sha256("manifest_sha256", signature.manifest_sha256.as_str())?;
    validate_sha256("artifact_sha256", context.artifact_sha256)?;
    let envelope = PluginReleaseSigningEnvelope {
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
    };
    serde_json::to_vec(&envelope).map_err(PluginSignatureVerificationError::SerializePayload)
}

pub fn verify_plugin_release_signature(
    context: PluginReleaseVerificationContext<'_>,
    manifest: &PluginManifest,
    signature: &PluginReleaseSignature,
    trusted_key: &SigningKeyRef,
) -> Result<(), PluginSignatureVerificationError> {
    validate_signing_key_usage(trusted_key, PLUGIN_SIGNING_KEY_USAGE_RELEASE)?;
    validate_release_identity(context, signature, trusted_key)?;
    validate_signing_key_window(signature.signed_at.as_str(), trusted_key)?;

    let expected_manifest_sha256 = normalized_plugin_manifest_sha256(manifest)
        .map_err(PluginSignatureVerificationError::SerializePayload)?;
    if signature.manifest_sha256 != expected_manifest_sha256 {
        return Err(PluginSignatureVerificationError::ManifestHashMismatch);
    }

    let public_key = decode_base64_field(
        "trusted_key.public_key_base64",
        trusted_key.public_key_base64.as_str(),
        32,
    )?;
    let signature_bytes = decode_base64_field(
        "signature.signature_base64",
        signature.signature_base64.as_str(),
        64,
    )?;
    let payload = plugin_release_signing_payload(context, signature)?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(payload.as_slice(), signature_bytes.as_slice())
        .map_err(|_| PluginSignatureVerificationError::InvalidSignature)
}

pub(super) fn validate_signing_key_usage(
    trusted_key: &SigningKeyRef,
    required_usage: &'static str,
) -> Result<(), PluginSignatureVerificationError> {
    if trusted_key.usages.is_empty()
        || trusted_key
            .usages
            .iter()
            .any(|usage| usage == required_usage)
    {
        Ok(())
    } else {
        Err(PluginSignatureVerificationError::InvalidCatalogDocument {
            message: format!(
                "signing key {} is not authorized for {required_usage}",
                trusted_key.key_id
            ),
        })
    }
}

fn validate_release_identity(
    context: PluginReleaseVerificationContext<'_>,
    signature: &PluginReleaseSignature,
    trusted_key: &SigningKeyRef,
) -> Result<(), PluginSignatureVerificationError> {
    if signature.algorithm != PLUGIN_SIGNATURE_ALGORITHM_ED25519 {
        return Err(PluginSignatureVerificationError::UnsupportedAlgorithm(
            signature.algorithm.clone(),
        ));
    }
    if signature.marketplace_id != context.marketplace_id {
        return Err(PluginSignatureVerificationError::IdentityMismatch {
            field: "marketplace_id",
        });
    }
    if signature.publisher_id != context.publisher_id {
        return Err(PluginSignatureVerificationError::IdentityMismatch {
            field: "publisher_id",
        });
    }
    if trusted_key.key_id != signature.key_id {
        return Err(PluginSignatureVerificationError::IdentityMismatch { field: "key_id" });
    }
    if trusted_key.publisher_id != context.publisher_id {
        return Err(PluginSignatureVerificationError::IdentityMismatch {
            field: "trusted_key.publisher_id",
        });
    }
    if trusted_key.algorithm != signature.algorithm {
        return Err(PluginSignatureVerificationError::IdentityMismatch {
            field: "trusted_key.algorithm",
        });
    }
    Ok(())
}

pub(super) fn validate_signing_key_window(
    signed_at: &str,
    trusted_key: &SigningKeyRef,
) -> Result<(), PluginSignatureVerificationError> {
    let signed_at = parse_timestamp("signature.signed_at", signed_at)?;
    let valid_from = parse_timestamp("trusted_key.valid_from", trusted_key.valid_from.as_str())?;
    let valid_until = trusted_key
        .valid_until
        .as_deref()
        .map(|value| parse_timestamp("trusted_key.valid_until", value))
        .transpose()?;
    if let Some(revoked_at) = trusted_key.revoked_at.as_deref() {
        parse_timestamp("trusted_key.revoked_at", revoked_at)?;
        return Err(PluginSignatureVerificationError::KeyRevoked);
    }
    if signed_at < valid_from || valid_until.is_some_and(|until| signed_at > until) {
        return Err(PluginSignatureVerificationError::OutsideKeyValidity);
    }
    Ok(())
}

pub(super) fn parse_timestamp(
    field: &'static str,
    value: &str,
) -> Result<DateTime<chrono::FixedOffset>, PluginSignatureVerificationError> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|_| PluginSignatureVerificationError::InvalidTimestamp { field })
}

pub(super) fn validate_sha256(
    field: &'static str,
    value: &str,
) -> Result<(), PluginSignatureVerificationError> {
    if value.len() != 64
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(PluginSignatureVerificationError::InvalidSha256 { field });
    }
    Ok(())
}

pub(super) fn decode_base64_field(
    field: &'static str,
    value: &str,
    expected: usize,
) -> Result<Vec<u8>, PluginSignatureVerificationError> {
    let decoded = STANDARD
        .decode(value.as_bytes())
        .map_err(|_| PluginSignatureVerificationError::InvalidBase64 { field })?;
    if decoded.len() != expected {
        return Err(PluginSignatureVerificationError::InvalidLength {
            field,
            actual: decoded.len(),
            expected,
        });
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests;
