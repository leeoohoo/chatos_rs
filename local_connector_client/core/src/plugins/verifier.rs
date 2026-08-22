// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, plugin_component_descriptors, verify_plugin_release_signature,
    PluginCatalogRecord, PluginComponentDescriptor, PluginDependencySpec, PluginInstallSource,
    PluginManifest, PluginMarketplaceRecord, PluginMcpServer, PluginPermissionRequirement,
    PluginReleaseRecord, PluginReleaseVerificationContext,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

use super::archive::{extract_npm_package, sha256_file, PluginPackageLimits, VerifiedPackageFiles};

const TRUST_LEVEL_TRUSTED: &str = "trusted";
const MAX_PACKAGE_JSON_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct PluginPackageVerificationRequest<'a> {
    pub marketplace: &'a PluginMarketplaceRecord,
    pub catalog: &'a PluginCatalogRecord,
    pub release: &'a PluginReleaseRecord,
    pub package_path: &'a Path,
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
pub struct VerifiedPluginPackage {
    pub root: PathBuf,
    pub manifest: PluginManifest,
    pub artifact_sha256: String,
    pub package_file_sha256: BTreeMap<String, String>,
    pub unpacked_bytes: u64,
    pub inventory: PluginRequirementInventory,
}

#[derive(Debug, Deserialize)]
struct NpmPackageJson {
    name: String,
    version: String,
    bin: NpmBin,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NpmBin {
    One(String),
    Many(BTreeMap<String, String>),
}

impl NpmPackageJson {
    fn bins(&self) -> BTreeMap<String, String> {
        match &self.bin {
            NpmBin::One(path) => BTreeMap::from([(
                unscoped_package_name(self.name.as_str()).to_string(),
                path.clone(),
            )]),
            NpmBin::Many(values) => values.clone(),
        }
    }
}

pub fn verify_plugin_package(
    request: PluginPackageVerificationRequest<'_>,
    limits: PluginPackageLimits,
) -> Result<VerifiedPluginPackage> {
    verify_plugin_install_source_records(request.marketplace, request.catalog, request.release)?;
    verify_npm_integrity(
        request.package_path,
        request.release.npm_package.integrity.as_str(),
    )?;
    let artifact_sha256 = sha256_file(request.package_path, limits.max_package_bytes)?;
    if artifact_sha256 != request.release.artifact_sha256 {
        bail!("downloaded npm package SHA-256 does not match the signed Release");
    }
    let extracted = extract_npm_package(request.package_path, request.extraction_root, limits)?;
    match finish_package_verification(request, extracted, artifact_sha256) {
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
        || release.npm_package.version != release.version
        || release.npm_package.name.trim().is_empty()
        || !release.npm_package.integrity.starts_with("sha512-")
        || catalog.name != release.normalized_manifest.name
        || release.manifest_schema_version != release.normalized_manifest.schema_version
        || release.dependencies != release.normalized_manifest.dependencies
        || release.permissions != release.normalized_manifest.permissions
        || release.supported_platforms
            != release.normalized_manifest.dependencies.supported_platforms
    {
        bail!("Plugin Release identity does not match its npm package and normalized Manifest");
    }
    Ok(())
}

fn finish_package_verification(
    request: PluginPackageVerificationRequest<'_>,
    extracted: VerifiedPackageFiles,
    artifact_sha256: String,
) -> Result<VerifiedPluginPackage> {
    let raw = read_limited_file(
        extracted.root.join("package.json").as_path(),
        MAX_PACKAGE_JSON_BYTES,
        "npm package.json",
    )?;
    let package: NpmPackageJson =
        serde_json::from_str(raw.as_str()).context("parse npm package.json")?;
    if package.name != request.release.npm_package.name
        || package.version != request.release.npm_package.version
    {
        bail!("npm package.json identity does not match the signed Plugin Release");
    }
    validate_declared_bins(
        &request.release.normalized_manifest,
        &package,
        &extracted.file_sha256,
    )?;
    let manifest = request.release.normalized_manifest.clone();
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
    Ok(VerifiedPluginPackage {
        root: extracted.root,
        manifest,
        artifact_sha256,
        package_file_sha256: extracted.file_sha256,
        unpacked_bytes: extracted.unpacked_bytes,
        inventory,
    })
}

fn validate_declared_bins(
    manifest: &PluginManifest,
    package: &NpmPackageJson,
    files: &BTreeMap<String, String>,
) -> Result<()> {
    let bins = package.bins();
    for server in &manifest.mcp_servers {
        let PluginMcpServer::Stdio { bin, .. } = server else {
            continue;
        };
        let path = bins
            .get(bin)
            .with_context(|| format!("npm package.json does not publish MCP bin: {bin}"))?;
        let normalized = normalize_plugin_relative_path(path)
            .map_err(anyhow::Error::msg)?
            .trim_start_matches("./")
            .to_string();
        if !files.contains_key(normalized.as_str()) {
            bail!("npm MCP bin is missing from the verified package: {bin}");
        }
    }
    Ok(())
}

fn verify_npm_integrity(path: &Path, integrity: &str) -> Result<()> {
    let encoded = integrity
        .trim()
        .strip_prefix("sha512-")
        .context("npm package integrity must use sha512")?;
    let expected = STANDARD
        .decode(encoded)
        .context("decode npm package sha512 integrity")?;
    let bytes = fs::read(path).context("read npm package for integrity verification")?;
    let actual = Sha512::digest(bytes.as_slice());
    if expected.as_slice() != actual.as_slice() {
        bail!("downloaded npm package does not match npm integrity");
    }
    Ok(())
}

fn read_limited_file(path: &Path, limit: u64, label: &str) -> Result<String> {
    let metadata = fs::symlink_metadata(path).with_context(|| format!("read {label} metadata"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        bail!("{label} is missing, unsafe, or exceeds its size limit");
    }
    fs::read_to_string(path).with_context(|| format!("read {label}"))
}

fn unscoped_package_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_package_identity_parser_ignores_untrusted_metadata() {
        let package: NpmPackageJson = serde_json::from_str(
            r#"{
                "name":"open-computer-use",
                "version":"0.3.1",
                "description":"Computer Use MCP",
                "license":"MIT",
                "repository":{"type":"git","url":"https://example.com/repository.git"},
                "scripts":{"postinstall":"node scripts/postinstall.mjs"},
                "bin":{"open-computer-use":"bin/open-computer-use"}
            }"#,
        )
        .expect("real-world npm package metadata");

        assert_eq!(package.name, "open-computer-use");
        assert_eq!(package.version, "0.3.1");
        assert_eq!(
            package.bins().get("open-computer-use").map(String::as_str),
            Some("bin/open-computer-use")
        );
    }
}
