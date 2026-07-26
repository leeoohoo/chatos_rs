// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, parse_plugin_manifest, plugin_component_descriptors,
    verify_plugin_release_signature, PluginCatalogRecord, PluginComponentDescriptor,
    PluginDependencySpec, PluginInstallSource, PluginManifest, PluginManifestSource,
    PluginMarketplaceRecord, PluginPermissionRequirement, PluginReleaseRecord,
    PluginReleaseVerificationContext,
};
use serde::{Deserialize, Serialize};

use super::archive::{
    extract_plugin_archive, sha256_file, PluginArchiveLimits, VerifiedArchiveFiles,
};

const TRUST_LEVEL_TRUSTED: &str = "trusted";
const CHECKSUM_SCHEMA_VERSION: u32 = 1;
const CODEX_MANIFEST_PATH: &str = ".codex-plugin/plugin.json";
const CHATOS_MANIFEST_PATH: &str = ".chatos-plugin/plugin.json";
const CODEX_CHECKSUMS_PATH: &str = ".codex-plugin/checksums.json";
const CHATOS_CHECKSUMS_PATH: &str = ".chatos-plugin/checksums.json";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_SBOM_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct PluginArtifactVerificationRequest<'a> {
    pub marketplace: &'a PluginMarketplaceRecord,
    pub catalog: &'a PluginCatalogRecord,
    pub release: &'a PluginReleaseRecord,
    pub archive_path: &'a Path,
    pub extraction_root: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginRequirementInventory {
    pub dependencies: PluginDependencySpec,
    pub permissions: Vec<PluginPermissionRequirement>,
    pub auth_component_keys: Vec<String>,
    pub components: Vec<PluginComponentDescriptor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedPluginArtifact {
    pub root: PathBuf,
    pub manifest: PluginManifest,
    pub manifest_source: PluginManifestSource,
    pub artifact_sha256: String,
    pub package_file_sha256: BTreeMap<String, String>,
    pub unpacked_bytes: u64,
    pub inventory: PluginRequirementInventory,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginChecksumIndex {
    schema_version: u32,
    files: BTreeMap<String, String>,
}

pub fn verify_plugin_artifact(
    request: PluginArtifactVerificationRequest<'_>,
    limits: PluginArchiveLimits,
) -> Result<VerifiedPluginArtifact> {
    verify_plugin_install_source_records(request.marketplace, request.catalog, request.release)?;

    let artifact_sha256 = sha256_file(request.archive_path, limits.max_archive_bytes)?;
    if artifact_sha256 != request.release.artifact_sha256 {
        bail!("downloaded Plugin artifact SHA-256 does not match the signed Release");
    }
    let extracted = extract_plugin_archive(request.archive_path, request.extraction_root, limits)?;
    match finish_artifact_verification(request, extracted, artifact_sha256) {
        Ok(verified) => Ok(verified),
        Err(error) => {
            let _ = fs::remove_dir_all(request.extraction_root);
            Err(error)
        }
    }
}

pub fn verify_plugin_install_source(source: &PluginInstallSource) -> Result<()> {
    verify_plugin_install_source_records(&source.marketplace, &source.catalog, &source.release)
}

pub(crate) fn verify_plugin_install_source_records(
    marketplace: &PluginMarketplaceRecord,
    catalog: &PluginCatalogRecord,
    release: &PluginReleaseRecord,
) -> Result<()> {
    validate_control_plane_records(marketplace, catalog, release)?;
    let key = marketplace
        .trusted_signing_keys
        .iter()
        .find(|key| key.key_id == release.signature.key_id)
        .context("Plugin Release signing key is not trusted by its Marketplace")?;
    verify_plugin_release_signature(
        PluginReleaseVerificationContext {
            plugin_id: catalog.id.as_str(),
            version: release.version.as_str(),
            marketplace_id: marketplace.id.as_str(),
            publisher_id: catalog.publisher.id.as_str(),
            artifact_sha256: release.artifact_sha256.as_str(),
        },
        &release.normalized_manifest,
        &release.signature,
        key,
    )
    .context("verify Plugin Release signature")
}

fn validate_control_plane_records(
    marketplace: &PluginMarketplaceRecord,
    catalog: &PluginCatalogRecord,
    release: &PluginReleaseRecord,
) -> Result<()> {
    if !marketplace.enabled || marketplace.trust_level != TRUST_LEVEL_TRUSTED {
        bail!("network Plugin installation requires an enabled trusted Marketplace");
    }
    if !catalog.enabled
        || catalog.marketplace_id != marketplace.id
        || release.plugin_id != catalog.id
    {
        bail!("Plugin Marketplace, Catalog, and Release identities are inconsistent");
    }
    if release.revoked_at.is_some() {
        bail!("revoked Plugin Releases cannot be installed");
    }
    if release.version != release.normalized_manifest.version
        || catalog.name != release.normalized_manifest.name
        || release.manifest_schema_version != release.normalized_manifest.schema_version
        || release.dependencies != release.normalized_manifest.dependencies
        || release.permissions != release.normalized_manifest.permissions
        || release.supported_platforms
            != release.normalized_manifest.dependencies.supported_platforms
    {
        bail!("Plugin Release identity does not match its normalized Manifest");
    }
    Ok(())
}

fn finish_artifact_verification(
    request: PluginArtifactVerificationRequest<'_>,
    extracted: VerifiedArchiveFiles,
    artifact_sha256: String,
) -> Result<VerifiedPluginArtifact> {
    let (manifest_path, source, checksum_path) = package_metadata_paths(&extracted)?;
    let manifest_raw = read_limited_file(
        extracted.root.join(manifest_path).as_path(),
        MAX_METADATA_BYTES,
        "Plugin Manifest",
    )?;
    let manifest = parse_plugin_manifest(manifest_raw.as_str(), source)
        .context("parse installed Plugin Manifest")?;
    if manifest != request.release.normalized_manifest {
        bail!("Plugin archive Manifest does not match the signed normalized Release Manifest");
    }

    let checksum_raw = read_limited_file(
        extracted.root.join(checksum_path).as_path(),
        MAX_METADATA_BYTES,
        "Plugin checksum index",
    )?;
    let checksum_index: PluginChecksumIndex =
        serde_json::from_str(checksum_raw.as_str()).context("parse Plugin checksum index")?;
    let package_file_sha256 = verify_checksum_index(&extracted, checksum_path, checksum_index)?;
    verify_relative_sbom(
        request.release,
        extracted.root.as_path(),
        &package_file_sha256,
    )?;

    let components = plugin_component_descriptors(&manifest);
    let auth_component_keys = manifest
        .apps
        .iter()
        .map(|app| app.component_key.clone())
        .collect();
    let inventory = PluginRequirementInventory {
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        auth_component_keys,
        components,
    };
    Ok(VerifiedPluginArtifact {
        root: extracted.root,
        manifest,
        manifest_source: source,
        artifact_sha256,
        package_file_sha256,
        unpacked_bytes: extracted.unpacked_bytes,
        inventory,
    })
}

fn package_metadata_paths(
    extracted: &VerifiedArchiveFiles,
) -> Result<(&'static str, PluginManifestSource, &'static str)> {
    let has_codex_manifest = extracted.file_sha256.contains_key(CODEX_MANIFEST_PATH);
    let has_chatos_manifest = extracted.file_sha256.contains_key(CHATOS_MANIFEST_PATH);
    match (has_codex_manifest, has_chatos_manifest) {
        (true, false) => {
            if !extracted.file_sha256.contains_key(CODEX_CHECKSUMS_PATH) {
                bail!("Codex Plugin archive is missing .codex-plugin/checksums.json");
            }
            Ok((
                CODEX_MANIFEST_PATH,
                PluginManifestSource::Codex,
                CODEX_CHECKSUMS_PATH,
            ))
        }
        (false, true) => {
            if !extracted.file_sha256.contains_key(CHATOS_CHECKSUMS_PATH) {
                bail!("ChatOS Plugin archive is missing .chatos-plugin/checksums.json");
            }
            Ok((
                CHATOS_MANIFEST_PATH,
                PluginManifestSource::Chatos,
                CHATOS_CHECKSUMS_PATH,
            ))
        }
        (true, true) => bail!("Plugin archive contains both Codex and ChatOS root Manifests"),
        (false, false) => bail!("Plugin archive is missing a root Plugin Manifest"),
    }
}

fn verify_checksum_index(
    extracted: &VerifiedArchiveFiles,
    checksum_path: &str,
    index: PluginChecksumIndex,
) -> Result<BTreeMap<String, String>> {
    if index.schema_version != CHECKSUM_SCHEMA_VERSION {
        bail!("unsupported Plugin checksum index schema version");
    }
    let mut normalized = BTreeMap::new();
    for (path, digest) in index.files {
        let normalized_path = normalize_plugin_relative_path(path.as_str())
            .map_err(anyhow::Error::msg)?
            .trim_start_matches("./")
            .to_string();
        if normalized_path == checksum_path || normalized.insert(normalized_path, digest).is_some()
        {
            bail!("Plugin checksum index contains a duplicate or self-referential path");
        }
    }
    let mut actual = extracted.file_sha256.clone();
    actual.remove(checksum_path);
    if normalized.keys().ne(actual.keys()) {
        bail!("Plugin checksum index must cover every package file exactly once");
    }
    for (path, actual_digest) in &actual {
        let expected_digest = normalized
            .get(path)
            .context("missing Plugin file checksum")?;
        if !is_sha256(expected_digest) || expected_digest != actual_digest {
            bail!("Plugin file checksum mismatch: {path}");
        }
    }
    Ok(extracted.file_sha256.clone())
}

fn verify_relative_sbom(
    release: &PluginReleaseRecord,
    root: &Path,
    files: &BTreeMap<String, String>,
) -> Result<()> {
    let sbom_ref = release
        .sbom_ref
        .as_deref()
        .context("network Plugin Release must declare an embedded SBOM")?;
    if sbom_ref.contains("://") {
        bail!("network Plugin Release SBOM must be embedded in the signed artifact");
    }
    let normalized = normalize_plugin_relative_path(sbom_ref).map_err(anyhow::Error::msg)?;
    let path = normalized.trim_start_matches("./");
    if !files.contains_key(path) {
        bail!("Plugin Release SBOM reference is missing from the verified package");
    }
    let raw = read_limited_file(root.join(path).as_path(), MAX_SBOM_BYTES, "Plugin SBOM")?;
    let document: serde_json::Value =
        serde_json::from_str(raw.as_str()).context("parse Plugin SBOM JSON")?;
    let is_cyclonedx = document
        .get("bomFormat")
        .and_then(serde_json::Value::as_str)
        == Some("CycloneDX")
        && document
            .get("specVersion")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
    let is_spdx = document
        .get("spdxVersion")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.starts_with("SPDX-"))
        && document
            .get("SPDXID")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
    if !is_cyclonedx && !is_spdx {
        bail!("Plugin SBOM must be CycloneDX JSON or SPDX JSON");
    }
    Ok(())
}

fn read_limited_file(path: &Path, max_bytes: u64, label: &str) -> Result<String> {
    let metadata = fs::metadata(path).with_context(|| format!("read {label} metadata"))?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        bail!("{label} is missing, not a file, or exceeds the size limit");
    }
    fs::read_to_string(path).with_context(|| format!("read UTF-8 {label}"))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .copied()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
