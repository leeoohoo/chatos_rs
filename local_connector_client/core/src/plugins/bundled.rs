// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, plugin_component_descriptors, validate_plugin_manifest,
    PluginAuthor, PluginComponentKind, PluginDependencySpec, PluginInstallStatus,
    PluginInterfaceMetadata, PluginManifest, PluginPathRef, PluginPermissionRequirement,
    PLUGIN_MANIFEST_SCHEMA_VERSION_V1,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::skills::{
    internal_skill_bundle_hash, internal_skill_catalog, internal_skill_instructions,
    internal_skill_manifest, InternalSkillCatalogItem,
};

use super::archive::{copy_verified_directory, verified_directory_files, PluginArchiveLimits};
use super::catalog::{bundled_plugin_spec, BundledPluginSpec, BUNDLED_SIGNATURE_KEY_ID};
use super::installer::write_installation_metadata;
use super::journal::{
    begin_transaction, transition_transaction, PluginTransactionOperation, PluginTransactionRecord,
};
use super::{
    InstalledPluginVersion, PluginInstallOutcome, PluginInstaller, PluginRequirementInventory,
};

const BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";
const BUNDLE_INDEX_FILE: &str = "plugin-bundle-index.json";
const MANIFEST_FILE: &str = ".chatos-plugin/plugin.json";
const CHECKSUM_FILE: &str = ".chatos-plugin/checksums.json";
const SBOM_FILE: &str = "sbom.spdx.json";
const MAX_BUNDLE_INDEX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledPluginBundleIndex {
    schema_version: u32,
    catalog_revision: String,
    release_version: String,
    release_epoch: String,
    artifact_revision: String,
    platform: String,
    plugins: Vec<BundledPluginBundleIndexEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledPluginBundleIndexEntry {
    plugin_id: String,
    release_id: String,
    name: String,
    version: String,
    published_at: String,
    artifact_revision: String,
    platform: String,
    relative_path: String,
    manifest_sha256: String,
    artifact_sha256: String,
    staged_content_sha256: String,
    skills: Vec<BundledPluginBundleSkillEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledPluginBundleSkillEntry {
    skill_id: String,
    bundle_id: String,
    name: String,
    version: String,
    bundle_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BundledChecksumIndex {
    schema_version: u32,
    files: BTreeMap<String, String>,
}

#[derive(Debug)]
struct VerifiedBundledPluginPackage {
    spec: BundledPluginSpec,
    source_path: PathBuf,
    artifact_sha256: String,
    manifest_sha256: String,
    package_file_sha256: BTreeMap<String, String>,
    inventory: PluginRequirementInventory,
}

impl PluginInstaller {
    pub fn install_bundled_directory(
        &self,
        bundled_root: &Path,
        plugin_id: &str,
    ) -> Result<PluginInstallOutcome> {
        let spec = bundled_plugin_spec(plugin_id)?;
        let _guard = self.operation_guard()?;
        self.ensure_upgrade_is_allowed(plugin_id, spec.release_version.as_str())?;
        let registry = self.registry()?;
        let from_version = registry
            .plugins
            .get(plugin_id)
            .and_then(|plugin| plugin.active_version.clone());
        let from_release_id = registry.plugins.get(plugin_id).and_then(|plugin| {
            plugin
                .active_version
                .as_deref()
                .and_then(|version| plugin.versions.get(version))
                .map(|version| version.release_id.clone())
        });
        let operation = if from_version.is_some() {
            PluginTransactionOperation::Update
        } else {
            PluginTransactionOperation::Install
        };
        let transaction_id = Uuid::new_v4().to_string();
        let relative_staging_path = format!(".staging/install-{transaction_id}");
        let relative_final_path = self.relative_installation_path(
            plugin_id,
            spec.name.as_str(),
            spec.release_version.as_str(),
        );
        let now = Utc::now().to_rfc3339();
        begin_transaction(
            self.plugin_root.as_path(),
            PluginTransactionRecord {
                transaction_id: transaction_id.clone(),
                operation,
                status: PluginInstallStatus::Verifying,
                plugin_id: plugin_id.to_string(),
                release_id: Some(spec.release_id.clone()),
                from_version,
                target_version: Some(spec.release_version.clone()),
                relative_staging_path: Some(relative_staging_path.clone()),
                relative_final_path: Some(relative_final_path.clone()),
                relative_storage_path: None,
                relative_trash_path: None,
                downloaded_bytes: 0,
                total_bytes: None,
                started_at: now.clone(),
                updated_at: now,
                completed_at: None,
                recovered_after_restart: false,
                last_error: None,
            },
        )?;
        let result =
            verify_bundled_plugin_package(bundled_root, spec, self.limits).and_then(|package| {
                self.install_verified_bundled_package(
                    package,
                    transaction_id.as_str(),
                    operation,
                    relative_staging_path.as_str(),
                    relative_final_path,
                )
            });
        let outcome = self.finish_operation(
            transaction_id.as_str(),
            result,
            PluginInstallStatus::Installed,
        )?;
        if let Some(release_id) = from_release_id {
            self.purge_release_credentials(plugin_id, release_id.as_str())
                .context(
                    "bundled Plugin update committed, but previous Release credential cleanup failed",
                )?;
        }
        Ok(outcome)
    }

    fn install_verified_bundled_package(
        &self,
        package: VerifiedBundledPluginPackage,
        transaction_id: &str,
        operation: PluginTransactionOperation,
        relative_staging_path: &str,
        relative_final_path: String,
    ) -> Result<PluginInstallOutcome> {
        let staging_path = self.plugin_root.join(relative_staging_path);
        let staging_parent = staging_path
            .parent()
            .context("bundled Plugin staging path has no parent")?;
        fs::create_dir_all(staging_parent).context("create bundled Plugin staging parent")?;
        fs::create_dir(&staging_path).context("create isolated bundled Plugin transaction")?;
        let staging = BundledStagingDirectory(staging_path);
        let payload = staging.0.join("payload");
        copy_verified_directory(
            package.source_path.as_path(),
            payload.as_path(),
            &package.package_file_sha256,
            self.limits,
        )?;
        let next_status = match operation {
            PluginTransactionOperation::Install => PluginInstallStatus::Installing,
            PluginTransactionOperation::Update => PluginInstallStatus::Updating,
            PluginTransactionOperation::Rollback | PluginTransactionOperation::Uninstall => {
                bail!("invalid bundled Plugin install transaction operation")
            }
        };
        transition_transaction(
            self.plugin_root.as_path(),
            transaction_id,
            next_status,
            Utc::now().to_rfc3339(),
        )?;
        let final_path = self.plugin_root.join(relative_final_path.as_str());
        if final_path.exists() {
            bail!(
                "immutable bundled Plugin version is already installed: {}",
                package.spec.release_version
            );
        }
        let installed_version = InstalledPluginVersion {
            release_id: package.spec.release_id.clone(),
            version: package.spec.release_version.clone(),
            artifact_sha256: package.artifact_sha256,
            manifest_sha256: package.manifest_sha256,
            signature_key_id: BUNDLED_SIGNATURE_KEY_ID.to_string(),
            relative_installation_path: relative_final_path,
            installed_at: Utc::now().to_rfc3339(),
            package_file_sha256: package.package_file_sha256,
            inventory: package.inventory,
        };
        write_installation_metadata(payload.as_path(), &installed_version)?;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create bundled Plugin immutable version parent: {}",
                    parent.display()
                )
            })?;
        }
        fs::rename(payload.as_path(), final_path.as_path()).with_context(|| {
            format!(
                "atomically move bundled Plugin into immutable storage: {}",
                final_path.display()
            )
        })?;
        self.activate_verified_version(
            package.spec.plugin_id.as_str(),
            BUNDLED_MARKETPLACE_ID,
            package.spec.name.as_str(),
            installed_version,
            final_path.as_path(),
        )
    }
}

fn verify_bundled_plugin_package(
    bundled_root: &Path,
    spec: BundledPluginSpec,
    limits: PluginArchiveLimits,
) -> Result<VerifiedBundledPluginPackage> {
    let root = fs::canonicalize(bundled_root).with_context(|| {
        format!(
            "resolve bundled Plugin resource directory: {}",
            bundled_root.display()
        )
    })?;
    let root_metadata = fs::symlink_metadata(root.as_path())?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        bail!("bundled Plugin resource root is not a regular directory");
    }
    let index_path = root.join(BUNDLE_INDEX_FILE);
    let index_metadata =
        fs::symlink_metadata(index_path.as_path()).context("read bundled Plugin index metadata")?;
    if !index_metadata.is_file()
        || index_metadata.file_type().is_symlink()
        || index_metadata.len() > MAX_BUNDLE_INDEX_BYTES
    {
        bail!("bundled Plugin index is missing or exceeds the size limit");
    }
    let index: BundledPluginBundleIndex = serde_json::from_slice(
        fs::read(index_path.as_path())
            .context("read bundled Plugin index")?
            .as_slice(),
    )
    .context("decode bundled Plugin index")?;
    validate_bundle_index(&index, &spec)?;
    let entry = index
        .plugins
        .iter()
        .find(|entry| entry.plugin_id == spec.plugin_id)
        .context("bundled Plugin package is missing from the staged index")?
        .clone();
    validate_bundle_index_entry(&entry, &spec, index.platform.as_str())?;

    let source_path = fs::canonicalize(root.join(entry.relative_path.as_str()))
        .context("resolve staged bundled Plugin directory")?;
    if !source_path.starts_with(root.as_path()) {
        bail!("staged bundled Plugin directory escapes the resource root");
    }
    let files = verified_directory_files(source_path.as_path(), limits)?;
    let (manifest, inventory, skills) = expected_manifest_and_inventory(&spec)?;
    let expected_paths = expected_package_paths(&skills);
    if files
        .file_sha256
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected_paths.iter().map(String::as_str).collect()
    {
        bail!("bundled Plugin package contains missing or unexpected files");
    }

    let manifest_raw = fs::read_to_string(source_path.join(MANIFEST_FILE))
        .context("read bundled Plugin Manifest")?;
    let staged_manifest: PluginManifest =
        serde_json::from_str(manifest_raw.as_str()).context("decode bundled Plugin Manifest")?;
    validate_plugin_manifest(&staged_manifest).context("validate bundled Plugin Manifest")?;
    if staged_manifest != manifest {
        bail!("bundled Plugin Manifest differs from the embedded Catalog inventory");
    }
    let manifest_sha256 =
        normalized_plugin_manifest_sha256(&manifest).context("hash bundled Plugin Manifest")?;
    if manifest_sha256 != entry.manifest_sha256 {
        bail!("bundled Plugin Manifest SHA-256 differs from the staged index");
    }

    verify_bundled_skill_files(source_path.as_path(), &skills)?;
    verify_bundled_sbom(
        source_path.as_path(),
        &spec,
        &skills,
        entry.artifact_sha256.as_str(),
    )?;
    verify_bundled_checksum_index(&files.file_sha256, source_path.as_path())?;
    if staged_content_sha256(source_path.as_path(), &files.file_sha256)?
        != entry.staged_content_sha256
    {
        bail!("bundled Plugin staged content SHA-256 differs from the index");
    }
    let artifact_sha256 = bundled_artifact_sha256(&spec, &skills);
    if artifact_sha256 != entry.artifact_sha256 {
        bail!("bundled Plugin artifact SHA-256 differs from the embedded inventory");
    }

    Ok(VerifiedBundledPluginPackage {
        spec,
        source_path,
        artifact_sha256,
        manifest_sha256,
        package_file_sha256: files.file_sha256,
        inventory,
    })
}

fn validate_bundle_index(index: &BundledPluginBundleIndex, spec: &BundledPluginSpec) -> Result<()> {
    if index.schema_version != 1
        || index.catalog_revision != spec.catalog_revision
        || index.platform != local_platform()
        || index.plugins.len() != 12
        || index.release_version.trim().is_empty()
        || index.release_epoch.trim().is_empty()
        || index.artifact_revision.trim().is_empty()
    {
        bail!("bundled Plugin index is incomplete or does not match this client build");
    }
    let mut plugin_ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for entry in &index.plugins {
        if !plugin_ids.insert(entry.plugin_id.as_str())
            || !names.insert(entry.name.as_str())
            || !paths.insert(entry.relative_path.as_str())
        {
            bail!("bundled Plugin index contains duplicate identities");
        }
    }
    Ok(())
}

fn validate_bundle_index_entry(
    entry: &BundledPluginBundleIndexEntry,
    spec: &BundledPluginSpec,
    platform: &str,
) -> Result<()> {
    let expected_relative_path = format!("internal/{}/{}", spec.name, spec.release_version);
    if entry.plugin_id != spec.plugin_id
        || entry.release_id != spec.release_id
        || entry.name != spec.name
        || entry.version != spec.release_version
        || entry.published_at != spec.release_epoch
        || entry.artifact_revision != spec.artifact_revision
        || entry.platform != platform
        || entry.relative_path != expected_relative_path
        || entry.skills.len() != spec.skill_ids.len()
    {
        bail!("bundled Plugin staged identity differs from the embedded Catalog");
    }
    Ok(())
}

fn expected_manifest_and_inventory(
    spec: &BundledPluginSpec,
) -> Result<(
    PluginManifest,
    PluginRequirementInventory,
    Vec<InternalSkillCatalogItem>,
)> {
    let catalog = internal_skill_catalog()?;
    let by_id = catalog
        .skills
        .into_iter()
        .map(|skill| (skill.skill_id.clone(), skill))
        .collect::<BTreeMap<_, _>>();
    let skills = spec
        .skill_ids
        .iter()
        .map(|skill_id| {
            by_id
                .get(skill_id)
                .cloned()
                .with_context(|| format!("bundled Plugin maps unknown Skill: {skill_id}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let permissions = bundled_permissions(&skills);
    let mut capabilities = skills
        .iter()
        .map(|skill| skill.entrypoint_kind.clone())
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    if capabilities.is_empty() {
        capabilities.push("skills".to_string());
    }
    let manifest = PluginManifest {
        schema_version: PLUGIN_MANIFEST_SCHEMA_VERSION_V1,
        name: spec.name.clone(),
        version: spec.release_version.clone(),
        description: spec.description.clone(),
        author: PluginAuthor {
            name: "ChatOS".to_string(),
            email: None,
            url: None,
        },
        homepage: None,
        repository: None,
        license: Some("LicenseRef-Pending-Redistribution-Review".to_string()),
        keywords: vec!["bundled".to_string(), "skills".to_string()],
        skills: skills
            .iter()
            .map(|skill| PluginPathRef::new(format!("./skills/{}", skill.name)))
            .collect(),
        mcp_servers: Vec::new(),
        apps: Vec::new(),
        commands: Vec::new(),
        agents: Vec::new(),
        hooks: Vec::new(),
        ui: Vec::new(),
        interface: PluginInterfaceMetadata {
            display_name: spec.display_name.clone(),
            short_description: spec.description.clone(),
            long_description: spec.description.clone(),
            developer_name: "ChatOS".to_string(),
            category: spec.category.clone(),
            capabilities,
            website_url: None,
            privacy_policy_url: None,
            terms_of_service_url: None,
            default_prompt: Vec::new(),
            brand_color: None,
            composer_icon: None,
            logo: None,
            logo_dark: None,
            screenshots: Vec::new(),
        },
        dependencies: PluginDependencySpec::default(),
        permissions: permissions.clone(),
        bundled_content_variant: Some("chatos-internal-skill-bundles-v2".to_string()),
    };
    let by_name = skills
        .iter()
        .map(|skill| (skill.name.as_str(), skill))
        .collect::<BTreeMap<_, _>>();
    let mut components = plugin_component_descriptors(&manifest);
    for component in &mut components {
        if component.kind != PluginComponentKind::SkillCollection {
            bail!("bundled internal Plugin contains a non-Skill component");
        }
        let skill = by_name
            .get(component.component_key.as_str())
            .context("bundled Plugin component does not map to an internal Skill")?;
        let bundle_hash = internal_skill_bundle_hash(skill);
        component.display_name = skill.display_name.clone();
        component.runtime_kind = skill.entrypoint_kind.clone();
        component
            .metadata
            .insert("skill_id".to_string(), json!(skill.skill_id));
        component
            .metadata
            .insert("bundle_id".to_string(), json!(skill.bundle_id));
        component
            .metadata
            .insert("bundle_hash".to_string(), json!(bundle_hash));
        component.metadata.insert(
            "implementation_status".to_string(),
            json!(skill.implementation_status),
        );
    }
    Ok((
        manifest,
        PluginRequirementInventory {
            dependencies: PluginDependencySpec::default(),
            permissions,
            auth_component_keys: Vec::new(),
            components,
        },
        skills,
    ))
}

fn bundled_permissions(skills: &[InternalSkillCatalogItem]) -> Vec<PluginPermissionRequirement> {
    let mut by_permission = BTreeMap::<String, Vec<String>>::new();
    for skill in skills {
        for permission in &skill.permissions {
            by_permission
                .entry(permission.clone())
                .or_default()
                .push(skill.name.clone());
        }
    }
    by_permission
        .into_iter()
        .map(|(permission, mut components)| {
            components.sort();
            components.dedup();
            PluginPermissionRequirement {
                permission,
                required: true,
                reason: Some("Required by bundled Skill components".to_string()),
                components,
            }
        })
        .collect()
}

fn expected_package_paths(skills: &[InternalSkillCatalogItem]) -> BTreeSet<String> {
    let mut paths = BTreeSet::from([
        MANIFEST_FILE.to_string(),
        CHECKSUM_FILE.to_string(),
        SBOM_FILE.to_string(),
    ]);
    for skill in skills {
        for file in ["SKILL.md", "instructions.md", "skill.json"] {
            paths.insert(format!("skills/{}/{file}", skill.name));
        }
    }
    paths
}

fn verify_bundled_skill_files(root: &Path, skills: &[InternalSkillCatalogItem]) -> Result<()> {
    for skill in skills {
        let skill_root = root.join("skills").join(skill.name.as_str());
        let manifest = internal_skill_manifest(skill.skill_id.as_str())
            .context("embedded bundled Skill manifest is missing")?;
        let instructions = internal_skill_instructions(skill.skill_id.as_str())
            .context("embedded bundled Skill instructions are missing")?;
        if fs::read(skill_root.join("skill.json"))? != manifest.as_bytes()
            || fs::read(skill_root.join("instructions.md"))? != instructions.as_bytes()
        {
            bail!("bundled Plugin Skill content differs from the embedded inventory");
        }
        let description = serde_json::to_string(if skill.description.is_empty() {
            skill.display_name.as_str()
        } else {
            skill.description.as_str()
        })?;
        let skill_document = format!(
            "---\nname: {}\ndescription: {}\ndisable-model-invocation: false\n---\n\n{}\n",
            skill.name,
            description,
            instructions.trim_end()
        );
        if fs::read(skill_root.join("SKILL.md"))? != skill_document.as_bytes() {
            bail!("bundled Plugin SKILL.md differs from the embedded inventory");
        }
    }
    Ok(())
}

fn verify_bundled_sbom(
    root: &Path,
    spec: &BundledPluginSpec,
    skills: &[InternalSkillCatalogItem],
    artifact_sha256: &str,
) -> Result<()> {
    let document: Value = serde_json::from_slice(
        fs::read(root.join(SBOM_FILE))
            .context("read bundled Plugin SBOM")?
            .as_slice(),
    )
    .context("decode bundled Plugin SBOM")?;
    let packages = skills
        .iter()
        .enumerate()
        .map(|(index, skill)| {
            json!({
                "name": skill.bundle_id,
                "SPDXID": format!("SPDXRef-Package-{}", index + 1),
                "versionInfo": skill.version,
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": false,
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": "NOASSERTION",
                "checksums": [{
                    "algorithm": "SHA256",
                    "checksumValue": internal_skill_bundle_hash(skill),
                }],
            })
        })
        .collect::<Vec<_>>();
    let expected = json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": format!("ChatOS bundled Plugin {} {}", spec.name, spec.release_version),
        "documentNamespace": format!(
            "https://chatos.local/spdx/{}/{}/{}",
            spec.name, spec.release_version, artifact_sha256
        ),
        "creationInfo": {
            "created": spec.release_epoch,
            "creators": ["Organization: ChatOS"],
        },
        "packages": packages,
    });
    if document != expected {
        bail!("bundled Plugin SBOM differs from the embedded inventory");
    }
    Ok(())
}

fn verify_bundled_checksum_index(actual: &BTreeMap<String, String>, root: &Path) -> Result<()> {
    let index: BundledChecksumIndex = serde_json::from_slice(
        fs::read(root.join(CHECKSUM_FILE))
            .context("read bundled Plugin checksum index")?
            .as_slice(),
    )
    .context("decode bundled Plugin checksum index")?;
    if index.schema_version != 1 {
        bail!("unsupported bundled Plugin checksum index schema version");
    }
    let mut expected = actual.clone();
    expected.remove(CHECKSUM_FILE);
    if index.files != expected {
        bail!("bundled Plugin checksum index does not cover exact package content");
    }
    Ok(())
}

fn staged_content_sha256(root: &Path, files: &BTreeMap<String, String>) -> Result<String> {
    let mut ordered_paths = Vec::with_capacity(files.len());
    collect_staged_paths(root, root, &mut ordered_paths)?;
    let payload = std::iter::once("chatos-staged-plugin-bundle-v1".to_string())
        .chain(ordered_paths.into_iter().map(|path| {
            let digest = files
                .get(path.as_str())
                .expect("verified staged path must have a digest");
            format!("{path}:{digest}")
        }))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(hex::encode(Sha256::digest(payload.as_bytes())))
}

fn collect_staged_paths(root: &Path, directory: &Path, paths: &mut Vec<String>) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("read staged Plugin directory: {}", directory.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            bail!("staged Plugin content contains a symlink or special file");
        }
        if metadata.is_dir() {
            collect_staged_paths(root, entry.path().as_path(), paths)?;
        } else {
            let relative = entry
                .path()
                .strip_prefix(root)
                .context("derive staged Plugin content path")?
                .to_str()
                .context("staged Plugin content path is not UTF-8")?
                .replace('\\', "/");
            paths.push(relative);
        }
    }
    Ok(())
}

fn bundled_artifact_sha256(
    spec: &BundledPluginSpec,
    skills: &[InternalSkillCatalogItem],
) -> String {
    let mut parts = skills
        .iter()
        .map(|skill| format!("{}:{}", skill.skill_id, internal_skill_bundle_hash(skill)))
        .collect::<Vec<_>>();
    parts.sort();
    let payload = format!(
        "chatos-bundled-plugin-release-v1\n{}\n{}\n{}",
        spec.name,
        spec.artifact_revision,
        parts.join("\n")
    );
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn local_platform() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "windows-arm64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "linux-arm64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else {
        "unknown"
    }
}

struct BundledStagingDirectory(PathBuf);

impl Drop for BundledStagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.as_path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn expected_bundled_inventory_matches_native_skill_contract() {
        let spec = bundled_plugin_spec("bundled-plugin-computer-use").expect("Computer Use spec");
        let (manifest, inventory, skills) =
            expected_manifest_and_inventory(&spec).expect("Computer Use inventory");
        assert_eq!(manifest.name, "computer-use");
        assert_eq!(skills.len(), 1);
        assert_eq!(inventory.components.len(), 1);
        let component = &inventory.components[0];
        assert_eq!(component.component_key, "computer-use");
        assert_eq!(component.runtime_kind, "native_adapter");
        assert_eq!(
            component.metadata.get("skill_id").and_then(Value::as_str),
            Some("internal_skill_computer_use")
        );
        let expected_bundle_hash = internal_skill_bundle_hash(&skills[0]);
        assert_eq!(
            component
                .metadata
                .get("bundle_hash")
                .and_then(Value::as_str),
            Some(expected_bundle_hash.as_str())
        );
    }

    #[test]
    #[ignore = "requires CHATOS_TEST_BUNDLED_PLUGINS_DIR staged by prepare-plugin-bundles.mjs"]
    fn installs_and_uninstalls_verified_staged_bundled_plugin() {
        let bundled_root = std::env::var_os("CHATOS_TEST_BUNDLED_PLUGINS_DIR")
            .map(PathBuf::from)
            .expect("CHATOS_TEST_BUNDLED_PLUGINS_DIR");
        let temp = TempDir::new().expect("temporary Plugin store");
        let installer = PluginInstaller::new(temp.path().join("plugins"));
        let installed = installer
            .install_bundled_directory(bundled_root.as_path(), "bundled-plugin-computer-use")
            .expect("install verified Computer Use Plugin");
        assert_eq!(
            installed.installed_version.release_id,
            "bundled-release-computer-use-1-19-0"
        );
        assert_eq!(installed.installed_version.version, "1.19.0");
        assert_eq!(
            installed.installed_version.signature_key_id,
            BUNDLED_SIGNATURE_KEY_ID
        );
        let component = &installed.installed_version.inventory.components[0];
        assert_eq!(component.runtime_kind, "native_adapter");
        assert_eq!(
            component.metadata.get("skill_id").and_then(Value::as_str),
            Some("internal_skill_computer_use")
        );
        assert!(installer
            .active_installation("bundled-plugin-computer-use")
            .expect("verify active bundled Plugin")
            .is_some());
        assert!(installer
            .uninstall("bundled-plugin-computer-use")
            .expect("uninstall bundled Plugin"));
        assert!(installer
            .active_installation("bundled-plugin-computer-use")
            .expect("read after uninstall")
            .is_none());
    }

    #[test]
    #[ignore = "requires CHATOS_TEST_BUNDLED_PLUGINS_DIR staged by prepare-plugin-bundles.mjs"]
    fn rejects_tampered_staged_bundled_plugin_and_records_rejection() {
        let source_root = std::env::var_os("CHATOS_TEST_BUNDLED_PLUGINS_DIR")
            .map(PathBuf::from)
            .expect("CHATOS_TEST_BUNDLED_PLUGINS_DIR");
        let fixture = TempDir::new().expect("temporary bundled fixture");
        fs::copy(
            source_root.join(BUNDLE_INDEX_FILE),
            fixture.path().join(BUNDLE_INDEX_FILE),
        )
        .expect("copy staged index");
        let relative = Path::new("internal/computer-use/1.19.0");
        let source = source_root.join(relative);
        let destination = fixture.path().join(relative);
        let files = verified_directory_files(source.as_path(), PluginArchiveLimits::default())
            .expect("verify source fixture");
        copy_verified_directory(
            source.as_path(),
            destination.as_path(),
            &files.file_sha256,
            PluginArchiveLimits::default(),
        )
        .expect("copy bundled fixture");
        std::fs::OpenOptions::new()
            .append(true)
            .open(destination.join("skills/computer-use/instructions.md"))
            .expect("open instructions for tamper")
            .write_all(b"\ntampered\n")
            .expect("tamper instructions");

        let store = TempDir::new().expect("temporary Plugin store");
        let installer = PluginInstaller::new(store.path().join("plugins"));
        let error = installer
            .install_bundled_directory(fixture.path(), "bundled-plugin-computer-use")
            .expect_err("tampered bundled Plugin must fail");
        assert!(
            error.to_string().contains("checksum")
                || error.to_string().contains("embedded inventory")
                || error.to_string().contains("staged content")
        );
        let status = installer.status_snapshot().expect("rejected status");
        assert!(status.registry.plugins.is_empty());
        assert_eq!(
            status.transactions.history.last().map(|item| item.status),
            Some(PluginInstallStatus::Rejected)
        );
    }

    #[test]
    #[ignore = "requires CHATOS_TEST_BUNDLED_PLUGINS_DIR staged by prepare-plugin-bundles.mjs"]
    fn updates_bundled_plugin_and_preserves_verified_rollback_target() {
        let bundled_root = std::env::var_os("CHATOS_TEST_BUNDLED_PLUGINS_DIR")
            .map(PathBuf::from)
            .expect("CHATOS_TEST_BUNDLED_PLUGINS_DIR");
        let store = TempDir::new().expect("temporary Plugin store");
        let installer = PluginInstaller::new(store.path().join("plugins"));
        let spec = bundled_plugin_spec("bundled-plugin-computer-use").expect("Computer Use spec");
        let (_, inventory, _) =
            expected_manifest_and_inventory(&spec).expect("Computer Use inventory");
        let previous_relative_path = "installed/computer-use--fixture/1.18.0";
        fs::create_dir_all(installer.plugin_root().join(previous_relative_path))
            .expect("create previous immutable version");
        let previous = InstalledPluginVersion {
            release_id: "bundled-release-computer-use-1-18-0".to_string(),
            version: "1.18.0".to_string(),
            artifact_sha256: "0".repeat(64),
            manifest_sha256: "1".repeat(64),
            signature_key_id: BUNDLED_SIGNATURE_KEY_ID.to_string(),
            relative_installation_path: previous_relative_path.to_string(),
            installed_at: "2026-07-25T00:00:00Z".to_string(),
            package_file_sha256: BTreeMap::new(),
            inventory,
        };
        crate::plugins::state::save_registry(
            installer.plugin_root(),
            &crate::plugins::LocalPluginRegistry {
                schema_version: 1,
                plugins: BTreeMap::from([(
                    spec.plugin_id.clone(),
                    crate::plugins::LocalInstalledPlugin {
                        plugin_id: spec.plugin_id.clone(),
                        marketplace_id: BUNDLED_MARKETPLACE_ID.to_string(),
                        plugin_name: spec.name.clone(),
                        active_version: Some(previous.version.clone()),
                        previous_version: None,
                        versions: BTreeMap::from([(previous.version.clone(), previous)]),
                    },
                )]),
            },
        )
        .expect("seed previous registry");

        let updated = installer
            .install_bundled_directory(bundled_root.as_path(), spec.plugin_id.as_str())
            .expect("update bundled Plugin");
        assert_eq!(updated.installed_version.version, "1.19.0");
        assert_eq!(updated.plugin.previous_version.as_deref(), Some("1.18.0"));
        let rolled_back = installer
            .rollback(spec.plugin_id.as_str())
            .expect("rollback to previous bundled version");
        assert_eq!(rolled_back.version.version, "1.18.0");
        let history = installer
            .status_snapshot()
            .expect("Plugin status")
            .transactions
            .history;
        assert!(history.iter().any(|item| {
            item.operation == PluginTransactionOperation::Update
                && item.status == PluginInstallStatus::Installed
        }));
        assert!(history.iter().any(|item| {
            item.operation == PluginTransactionOperation::Rollback
                && item.status == PluginInstallStatus::Installed
        }));
    }

    #[test]
    #[ignore = "requires CHATOS_TEST_BUNDLED_PLUGINS_DIR staged by prepare-plugin-bundles.mjs"]
    fn installs_every_staged_bundled_plugin_with_exact_release_identity() {
        let bundled_root = std::env::var_os("CHATOS_TEST_BUNDLED_PLUGINS_DIR")
            .map(PathBuf::from)
            .expect("CHATOS_TEST_BUNDLED_PLUGINS_DIR");
        let index: BundledPluginBundleIndex = serde_json::from_slice(
            fs::read(bundled_root.join(BUNDLE_INDEX_FILE))
                .expect("read staged index")
                .as_slice(),
        )
        .expect("decode staged index");
        let store = TempDir::new().expect("temporary Plugin store");
        let installer = PluginInstaller::new(store.path().join("plugins"));
        for entry in &index.plugins {
            let installed = installer
                .install_bundled_directory(bundled_root.as_path(), entry.plugin_id.as_str())
                .unwrap_or_else(|error| panic!("install {}: {error:#}", entry.plugin_id));
            assert_eq!(installed.installed_version.release_id, entry.release_id);
            assert_eq!(installed.installed_version.version, entry.version);
            assert_eq!(
                installed.installed_version.artifact_sha256,
                entry.artifact_sha256
            );
        }
        let registry = installer.registry().expect("installed bundled registry");
        assert_eq!(registry.plugins.len(), 12);
        assert!(registry.plugins.values().all(|plugin| {
            plugin.marketplace_id == BUNDLED_MARKETPLACE_ID
                && plugin.active_version.is_some()
                && plugin.previous_version.is_none()
        }));
    }
}
