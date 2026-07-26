// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use ring::signature::{UnparsedPublicKey, ED25519};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::plugin_manifest::normalize_plugin_relative_path;
use crate::plugin_runtime::{
    plugin_component_descriptors, PluginCatalogRecord, PluginComponentSnapshot, PluginReleaseRecord,
};

use super::{
    decode_base64_field, parse_timestamp, validate_sha256, validate_signing_key_usage,
    validate_signing_key_window, verify_plugin_release_signature, PluginReleaseVerificationContext,
    PluginSignatureVerificationError, SigningKeyRef, PLUGIN_SIGNATURE_ALGORITHM_ED25519,
    PLUGIN_SIGNING_KEY_USAGE_CATALOG, PLUGIN_SIGNING_KEY_USAGE_RELEASE,
};

pub const PLUGIN_CATALOG_SIGNATURE_PURPOSE_V1: &str = "chatos.plugin.catalog.v1";
pub const PLUGIN_CATALOG_SCHEMA_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCatalogSignature {
    pub key_id: String,
    pub marketplace_id: String,
    pub algorithm: String,
    pub signature_base64: String,
    pub signed_at: String,
    pub catalog_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCatalogDocument {
    pub schema_version: u32,
    pub marketplace_id: String,
    pub revision: String,
    pub issued_at: String,
    #[serde(default)]
    pub signing_keys: Vec<SigningKeyRef>,
    #[serde(default)]
    pub plugins: Vec<PluginCatalogRecord>,
    #[serde(default)]
    pub releases: Vec<PluginReleaseRecord>,
    #[serde(default)]
    pub component_snapshots: Vec<PluginComponentSnapshot>,
    #[serde(default)]
    pub revoked_release_ids: Vec<String>,
    pub signature: PluginCatalogSignature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginCatalogVerificationContext<'a> {
    pub marketplace_id: &'a str,
    pub revision: &'a str,
    pub issued_at: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginCatalogSigningEnvelope<'a> {
    schema_version: u32,
    purpose: &'static str,
    marketplace_id: &'a str,
    revision: &'a str,
    issued_at: &'a str,
    key_id: &'a str,
    algorithm: &'a str,
    signed_at: &'a str,
    catalog_sha256: &'a str,
}

#[derive(Debug, Serialize)]
struct NormalizedPluginCatalogPayload {
    schema_version: u32,
    marketplace_id: String,
    revision: String,
    issued_at: String,
    signing_keys: Vec<SigningKeyRef>,
    plugins: Vec<PluginCatalogRecord>,
    releases: Vec<PluginReleaseRecord>,
    component_snapshots: Vec<PluginComponentSnapshot>,
    revoked_release_ids: Vec<String>,
}

pub fn normalized_plugin_catalog_sha256(
    document: &PluginCatalogDocument,
) -> Result<String, PluginSignatureVerificationError> {
    let normalized = normalized_catalog_payload(document)?;
    serde_json::to_vec(&normalized)
        .map(|payload| hex::encode(Sha256::digest(payload)))
        .map_err(PluginSignatureVerificationError::SerializePayload)
}

pub fn plugin_catalog_signing_payload(
    context: PluginCatalogVerificationContext<'_>,
    signature: &PluginCatalogSignature,
) -> Result<Vec<u8>, PluginSignatureVerificationError> {
    validate_sha256("catalog_sha256", signature.catalog_sha256.as_str())?;
    let envelope = PluginCatalogSigningEnvelope {
        schema_version: PLUGIN_CATALOG_SCHEMA_VERSION_V1,
        purpose: PLUGIN_CATALOG_SIGNATURE_PURPOSE_V1,
        marketplace_id: context.marketplace_id,
        revision: context.revision,
        issued_at: context.issued_at,
        key_id: signature.key_id.as_str(),
        algorithm: signature.algorithm.as_str(),
        signed_at: signature.signed_at.as_str(),
        catalog_sha256: signature.catalog_sha256.as_str(),
    };
    serde_json::to_vec(&envelope).map_err(PluginSignatureVerificationError::SerializePayload)
}

pub fn verify_plugin_catalog_signature(
    context: PluginCatalogVerificationContext<'_>,
    signature: &PluginCatalogSignature,
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
    if trusted_key.key_id != signature.key_id {
        return Err(PluginSignatureVerificationError::IdentityMismatch { field: "key_id" });
    }
    if trusted_key.algorithm != signature.algorithm {
        return Err(PluginSignatureVerificationError::IdentityMismatch {
            field: "trusted_key.algorithm",
        });
    }
    validate_signing_key_usage(trusted_key, PLUGIN_SIGNING_KEY_USAGE_CATALOG)?;
    validate_signing_key_window(signature.signed_at.as_str(), trusted_key)?;
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
    let payload = plugin_catalog_signing_payload(context, signature)?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(payload.as_slice(), signature_bytes.as_slice())
        .map_err(|_| PluginSignatureVerificationError::InvalidSignature)
}

pub fn verify_plugin_catalog_document(
    document: &PluginCatalogDocument,
    current_trusted_keys: &[SigningKeyRef],
) -> Result<(), PluginSignatureVerificationError> {
    let expected_sha256 = normalized_plugin_catalog_sha256(document)?;
    if document.signature.catalog_sha256 != expected_sha256 {
        return Err(PluginSignatureVerificationError::CatalogHashMismatch);
    }
    let catalog_key = current_trusted_keys
        .iter()
        .find(|key| key.key_id == document.signature.key_id)
        .ok_or_else(|| invalid_catalog("Catalog signing key is not in the current trust root"))?;
    verify_plugin_catalog_signature(
        PluginCatalogVerificationContext {
            marketplace_id: document.marketplace_id.as_str(),
            revision: document.revision.as_str(),
            issued_at: document.issued_at.as_str(),
        },
        &document.signature,
        catalog_key,
    )?;

    for release in document
        .releases
        .iter()
        .filter(|release| release.revoked_at.is_none())
    {
        let plugin = document
            .plugins
            .iter()
            .find(|plugin| plugin.id == release.plugin_id)
            .ok_or_else(|| invalid_catalog("Release references an unknown Plugin"))?;
        let release_key = document
            .signing_keys
            .iter()
            .find(|key| key.key_id == release.signature.key_id)
            .ok_or_else(|| invalid_catalog("Release signing key is absent from the Catalog"))?;
        verify_plugin_release_signature(
            PluginReleaseVerificationContext {
                plugin_id: plugin.id.as_str(),
                version: release.version.as_str(),
                marketplace_id: document.marketplace_id.as_str(),
                publisher_id: plugin.publisher.id.as_str(),
                artifact_sha256: release.artifact_sha256.as_str(),
            },
            &release.normalized_manifest,
            &release.signature,
            release_key,
        )?;
    }
    Ok(())
}

pub fn verify_plugin_catalog_update(
    document: &PluginCatalogDocument,
    current_trusted_keys: &[SigningKeyRef],
    previous_revision: Option<&str>,
    previous_issued_at: Option<&str>,
) -> Result<(), PluginSignatureVerificationError> {
    verify_plugin_catalog_document(document, current_trusted_keys)?;
    if previous_revision.is_some_and(|revision| revision == document.revision) {
        return Err(invalid_catalog(
            "Catalog update reuses the current revision",
        ));
    }
    if let Some(previous_issued_at) = previous_issued_at {
        let previous = parse_timestamp("catalog.previous_issued_at", previous_issued_at)?;
        let next = parse_timestamp("catalog.issued_at", document.issued_at.as_str())?;
        if next <= previous {
            return Err(invalid_catalog(
                "Catalog update does not advance the signed issue time",
            ));
        }
    }
    Ok(())
}

fn normalized_catalog_payload(
    document: &PluginCatalogDocument,
) -> Result<NormalizedPluginCatalogPayload, PluginSignatureVerificationError> {
    if document.schema_version != PLUGIN_CATALOG_SCHEMA_VERSION_V1 {
        return Err(invalid_catalog("unsupported schema_version"));
    }
    if document.marketplace_id.trim().is_empty()
        || document.revision.trim().is_empty()
        || document.issued_at.trim().is_empty()
    {
        return Err(invalid_catalog(
            "marketplace_id, revision, and issued_at are required",
        ));
    }
    parse_timestamp("catalog.issued_at", document.issued_at.as_str())?;
    if document.signature.marketplace_id != document.marketplace_id {
        return Err(invalid_catalog(
            "signature marketplace_id does not match the Catalog",
        ));
    }

    let mut signing_keys = document.signing_keys.clone();
    for key in &mut signing_keys {
        key.usages.sort();
        key.usages.dedup();
    }
    signing_keys.sort_by(|left, right| left.key_id.cmp(&right.key_id));
    ensure_unique(
        signing_keys.iter().map(|key| key.key_id.as_str()),
        "duplicate signing key ID",
    )?;
    for key in &signing_keys {
        if key.key_id.trim().is_empty()
            || key.publisher_id.trim().is_empty()
            || key.algorithm != PLUGIN_SIGNATURE_ALGORITHM_ED25519
            || key.usages.is_empty()
            || key.usages.iter().any(|usage| {
                !matches!(
                    usage.as_str(),
                    PLUGIN_SIGNING_KEY_USAGE_CATALOG | PLUGIN_SIGNING_KEY_USAGE_RELEASE
                )
            })
        {
            return Err(invalid_catalog(
                "Catalog signing keys require explicit supported usages",
            ));
        }
        decode_base64_field(
            "catalog.signing_keys.public_key_base64",
            key.public_key_base64.as_str(),
            32,
        )?;
        let valid_from =
            parse_timestamp("catalog.signing_keys.valid_from", key.valid_from.as_str())?;
        if let Some(value) = key.valid_until.as_deref() {
            let valid_until = parse_timestamp("catalog.signing_keys.valid_until", value)?;
            if valid_until <= valid_from {
                return Err(invalid_catalog(
                    "signing key valid_until must be later than valid_from",
                ));
            }
        }
        if let Some(value) = key.revoked_at.as_deref() {
            parse_timestamp("catalog.signing_keys.revoked_at", value)?;
        }
    }

    let mut plugins = document.plugins.clone();
    plugins.sort_by(|left, right| {
        left.plugin_key
            .cmp(&right.plugin_key)
            .then(left.id.cmp(&right.id))
    });
    ensure_unique(
        plugins.iter().map(|plugin| plugin.id.as_str()),
        "duplicate Plugin ID",
    )?;
    ensure_unique(
        plugins.iter().map(|plugin| plugin.plugin_key.as_str()),
        "duplicate Plugin key",
    )?;
    if plugins
        .iter()
        .any(|plugin| plugin.marketplace_id != document.marketplace_id)
    {
        return Err(invalid_catalog("Plugin belongs to a different Marketplace"));
    }
    for plugin in &plugins {
        if plugin.id.trim().is_empty()
            || plugin.name.trim().is_empty()
            || plugin.plugin_key != format!("{}@{}", plugin.name, document.marketplace_id)
        {
            return Err(invalid_catalog(
                "Plugin identity is incomplete or inconsistent",
            ));
        }
        parse_timestamp("catalog.plugins.created_at", plugin.created_at.as_str())?;
        parse_timestamp("catalog.plugins.updated_at", plugin.updated_at.as_str())?;
    }

    let mut releases = document.releases.clone();
    releases.sort_by(|left, right| {
        left.plugin_id
            .cmp(&right.plugin_id)
            .then(left.version.cmp(&right.version))
            .then(left.id.cmp(&right.id))
    });
    ensure_unique(
        releases.iter().map(|release| release.id.as_str()),
        "duplicate Release ID",
    )?;
    let mut release_coordinates = releases
        .iter()
        .map(|release| format!("{}@{}", release.plugin_id, release.version))
        .collect::<Vec<_>>();
    release_coordinates.sort();
    ensure_unique(
        release_coordinates.iter().map(String::as_str),
        "duplicate Plugin Release version",
    )?;
    for release in &releases {
        let plugin = plugins
            .iter()
            .find(|plugin| plugin.id == release.plugin_id)
            .ok_or_else(|| invalid_catalog("Release references an unknown Plugin"))?;
        Version::parse(release.version.as_str())
            .map_err(|_| invalid_catalog("Release version must use strict SemVer"))?;
        if release.version != release.normalized_manifest.version
            || plugin.name != release.normalized_manifest.name
            || release.manifest_schema_version != release.normalized_manifest.schema_version
            || release.components != plugin_component_descriptors(&release.normalized_manifest)
            || release.dependencies != release.normalized_manifest.dependencies
            || release.permissions != release.normalized_manifest.permissions
            || release.supported_platforms
                != release.normalized_manifest.dependencies.supported_platforms
        {
            return Err(invalid_catalog(
                "Release identity or requirements differ from its normalized Manifest",
            ));
        }
        if !matches!(
            release.release_channel.as_str(),
            "stable" | "beta" | "canary"
        ) {
            return Err(invalid_catalog("unsupported Release channel"));
        }
        parse_timestamp(
            "catalog.releases.published_at",
            release.published_at.as_str(),
        )?;
        if let Some(revoked_at) = release.revoked_at.as_deref() {
            parse_timestamp("catalog.releases.revoked_at", revoked_at)?;
        }
        if release.revoked_at.is_none() {
            let sbom_ref = release
                .sbom_ref
                .as_deref()
                .ok_or_else(|| invalid_catalog("active Release is missing an embedded SBOM"))?;
            if sbom_ref.contains("://") || normalize_plugin_relative_path(sbom_ref).is_err() {
                return Err(invalid_catalog(
                    "active Release SBOM must be a Plugin-relative artifact path",
                ));
            }
        }
    }
    for plugin in &plugins {
        if !plugin.latest_release_id.is_empty()
            && !releases.iter().any(|release| {
                release.id == plugin.latest_release_id
                    && release.plugin_id == plugin.id
                    && release.release_channel == "stable"
                    && release.revoked_at.is_none()
            })
        {
            return Err(invalid_catalog(
                "Plugin latest_release_id does not reference its own active stable Release",
            ));
        }
    }

    let mut component_snapshots = document.component_snapshots.clone();
    component_snapshots.sort_by(|left, right| {
        left.plugin_id
            .cmp(&right.plugin_id)
            .then(left.release_id.cmp(&right.release_id))
            .then(
                left.component
                    .component_key
                    .cmp(&right.component.component_key),
            )
    });
    let snapshot_coordinates = component_snapshots
        .iter()
        .map(|snapshot| {
            format!(
                "{}/{}/{}",
                snapshot.plugin_id, snapshot.release_id, snapshot.component.component_key
            )
        })
        .collect::<Vec<_>>();
    ensure_unique(
        snapshot_coordinates.iter().map(String::as_str),
        "duplicate Plugin component snapshot",
    )?;
    for snapshot in &component_snapshots {
        validate_sha256(
            "component_snapshot.content_sha256",
            &snapshot.content_sha256,
        )?;
        let release = releases
            .iter()
            .find(|release| {
                release.id == snapshot.release_id && release.plugin_id == snapshot.plugin_id
            })
            .ok_or_else(|| invalid_catalog("component snapshot references an unknown Release"))?;
        if !release.components.iter().any(|component| {
            component.component_key == snapshot.component.component_key
                && component == &snapshot.component
        }) {
            return Err(invalid_catalog(
                "component snapshot differs from its immutable Release component",
            ));
        }
    }
    for release in &releases {
        for component in &release.components {
            if !component_snapshots.iter().any(|snapshot| {
                snapshot.plugin_id == release.plugin_id
                    && snapshot.release_id == release.id
                    && snapshot.component == *component
            }) {
                return Err(invalid_catalog(
                    "Release component is missing an immutable content snapshot",
                ));
            }
        }
    }

    let mut revoked_release_ids = document.revoked_release_ids.clone();
    revoked_release_ids.sort();
    ensure_unique(
        revoked_release_ids.iter().map(String::as_str),
        "duplicate revoked Release ID",
    )?;
    let expected_revoked = releases
        .iter()
        .filter(|release| release.revoked_at.is_some())
        .map(|release| release.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if revoked_release_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        != expected_revoked
    {
        return Err(invalid_catalog(
            "revoked_release_ids does not match revoked Release records",
        ));
    }

    Ok(NormalizedPluginCatalogPayload {
        schema_version: document.schema_version,
        marketplace_id: document.marketplace_id.clone(),
        revision: document.revision.clone(),
        issued_at: document.issued_at.clone(),
        signing_keys,
        plugins,
        releases,
        component_snapshots,
        revoked_release_ids,
    })
}

fn ensure_unique<'a>(
    values: impl IntoIterator<Item = &'a str>,
    message: &str,
) -> Result<(), PluginSignatureVerificationError> {
    let mut seen = std::collections::HashSet::new();
    if values.into_iter().any(|value| !seen.insert(value)) {
        return Err(invalid_catalog(message));
    }
    Ok(())
}

fn invalid_catalog(message: impl Into<String>) -> PluginSignatureVerificationError {
    PluginSignatureVerificationError::InvalidCatalogDocument {
        message: message.into(),
    }
}
