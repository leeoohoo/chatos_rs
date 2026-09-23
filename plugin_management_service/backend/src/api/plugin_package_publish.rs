// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::{Extension, Json};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, normalized_plugin_manifest_sha256, parse_plugin_manifest,
    parse_skill_document, plugin_component_descriptors, plugin_release_signing_payload,
    plugin_skill_snapshot_sha256, skill_resource_manifest_sha256, PluginLicenseMetadata,
    PluginManifest, PluginMcpServer, PluginNpmPackage, PluginPublisher, PluginReleaseSignature,
    PluginReleaseVerificationContext, PluginSkillComponentSnapshot, PluginUiRuntime,
    RuntimeSkillResourceDescriptor, SigningKeyRef, SkillResourceKind,
    PLUGIN_SIGNATURE_ALGORITHM_ED25519, PLUGIN_SIGNING_KEY_USAGE_RELEASE,
    SKILL_RUNTIME_PROTOCOL_VERSION,
};
use flate2::read::GzDecoder;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::plugin_publishers::ensure_admin_managed_publisher;
use super::plugin_releases::publish_plugin_release_from_manifest;
use super::plugins::publish_plugin_catalog_entry;
use super::*;

const PACKAGE_JSON_PATH: &str = "package/package.json";
const MANIFEST_PATHS: &[&str] = &[
    "package/chatos.plugin.json",
    "package/.chatos-plugin/plugin.json",
    "package/plugin.json",
];
const MAX_PACKAGE_METADATA_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 50_000;
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_SKILL_INSTRUCTIONS_BYTES: u64 = 256 * 1024;
const MAX_SKILL_RESOURCE_BYTES: u64 = 1024 * 1024;
const MAX_SKILL_TOTAL_RESOURCE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SKILL_RESOURCE_COUNT: usize = 256;

#[derive(Debug, Serialize, Deserialize)]
struct StoredPluginArtifactMetadata {
    artifact_sha256: String,
    artifact_ref: String,
    npm_package: PluginNpmPackage,
    normalized_manifest: PluginManifest,
    #[serde(default)]
    skill_snapshots: Vec<PluginSkillComponentSnapshot>,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginPackageAnalysis {
    artifact_sha256: String,
    artifact_ref: String,
    package_name: String,
    package_version: String,
    npm_integrity: String,
    package_bins: Vec<String>,
    has_ui: bool,
    manifest: PluginManifest,
    components: Vec<chatos_plugin_management_sdk::PluginComponentDescriptor>,
    skill_snapshots: Vec<PluginSkillComponentSnapshot>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PublishUploadedPluginRequest {
    artifact_sha256: String,
    marketplace_id: String,
    publisher_id: String,
    #[serde(default)]
    publisher_name: Option<String>,
    #[serde(default)]
    publisher_website: Option<String>,
    license_id: String,
    #[serde(default)]
    license_url: Option<String>,
    #[serde(default)]
    redistributable: bool,
    #[serde(default = "default_public_visibility")]
    visibility: String,
    #[serde(default)]
    featured: bool,
    #[serde(default = "default_stable_channel")]
    release_channel: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PublishUploadedPluginResponse {
    catalog: PluginCatalogRecord,
    release: PluginReleaseRecord,
}

#[derive(Debug, Deserialize)]
struct NpmPackageJson {
    name: String,
    version: String,
    #[serde(default)]
    bin: Value,
}

#[derive(Debug)]
struct ParsedPackage {
    package_name: String,
    package_version: String,
    package_bins: Vec<String>,
    manifest: PluginManifest,
    skill_snapshots: Vec<PluginSkillComponentSnapshot>,
}

#[derive(Debug)]
struct PackageBin {
    name: String,
    archive_path: String,
}

pub(super) async fn analyze_plugin_package(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    mut multipart: Multipart,
) -> Result<Json<PluginPackageAnalysis>, ApiError> {
    ensure_super_admin(&user)?;
    let mut package_bytes = None;
    let mut manifest_override = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::bad_request(format!("read upload field failed: {error}")))?
    {
        match field.name() {
            Some("package") => {
                let bytes = field.bytes().await.map_err(|error| {
                    ApiError::bad_request(format!("read npm package upload failed: {error}"))
                })?;
                if bytes.is_empty() || bytes.len() > state.config.plugin_artifact_max_bytes {
                    return Err(ApiError::bad_request(format!(
                        "npm package must contain 1-{} bytes",
                        state.config.plugin_artifact_max_bytes
                    )));
                }
                package_bytes = Some(bytes.to_vec());
            }
            Some("manifest") => {
                let text = field.text().await.map_err(|error| {
                    ApiError::bad_request(format!("read Plugin Manifest upload failed: {error}"))
                })?;
                if !text.trim().is_empty() {
                    manifest_override = Some(text);
                }
            }
            _ => {}
        }
    }
    let package_bytes = package_bytes
        .ok_or_else(|| ApiError::bad_request("package multipart field is required"))?;
    let analysis_state = state.clone();
    let analysis = tokio::task::spawn_blocking(move || {
        persist_and_analyze_package(&analysis_state, package_bytes, manifest_override)
    })
    .await
    .map_err(|error| {
        ApiError::internal(format!("Plugin package analysis task failed: {error}"))
    })??;
    Ok(Json(analysis))
}

pub(super) async fn publish_uploaded_plugin(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(mut request): Json<PublishUploadedPluginRequest>,
) -> Result<Json<PublishUploadedPluginResponse>, ApiError> {
    ensure_super_admin(&user)?;
    request.artifact_sha256 =
        normalize_sha256(request.artifact_sha256.as_str(), "artifact_sha256")?;
    request.marketplace_id =
        validate_plugin_identifier(request.marketplace_id.as_str(), "marketplace_id")?;
    request.publisher_id =
        validate_plugin_identifier(request.publisher_id.as_str(), "publisher_id")?;
    request.license_id = required_text(Some(request.license_id.as_str()), "license_id")?;
    request.visibility = normalize_plugin_visibility(request.visibility.as_str())?;
    request.release_channel = normalize_release_channel(request.release_channel.as_str())?;
    let license_url = normalize_optional_https_url(request.license_url.as_deref())?;

    let verification_state = state.clone();
    let verification_sha256 = request.artifact_sha256.clone();
    let stored = tokio::task::spawn_blocking(move || {
        let stored =
            read_stored_artifact_metadata(&verification_state, verification_sha256.as_str())?;
        verify_stored_artifact(&verification_state, &stored)?;
        Ok::<_, ApiError>(stored)
    })
    .await
    .map_err(|error| {
        ApiError::internal(format!("Plugin artifact verification task failed: {error}"))
    })??;
    let marketplace = state
        .store
        .get_plugin_marketplace(request.marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Plugin marketplace not found"))?;
    if !marketplace.enabled
        || marketplace.trust_level != PLUGIN_TRUST_TRUSTED
        || marketplace.source_kind != PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
    {
        return Err(ApiError::conflict(
            "uploaded Plugin publishing requires an enabled trusted admin_registry marketplace",
        ));
    }
    let existing_catalog = state
        .store
        .find_plugin_catalog_entry(
            marketplace.id.as_str(),
            stored.normalized_manifest.name.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    if let Some(catalog) = existing_catalog.as_ref() {
        if catalog.publisher.id != request.publisher_id {
            return Err(ApiError::conflict(
                "existing Plugin catalog entry belongs to another publisher",
            ));
        }
        if state
            .store
            .find_plugin_release_by_version(
                catalog.id.as_str(),
                stored.normalized_manifest.version.as_str(),
            )
            .await
            .map_err(ApiError::internal)?
            .is_some()
        {
            return Err(ApiError::conflict(
                "Plugin release version is immutable and already exists",
            ));
        }
    }
    let publisher = ensure_admin_managed_publisher(
        &state,
        &user,
        &marketplace,
        request.publisher_id.as_str(),
        request.publisher_name.as_deref(),
        request.publisher_website.as_deref(),
    )
    .await?;
    let (managed_key, key_pair) =
        ensure_managed_release_key(&state, &marketplace, &publisher).await?;

    let plugin_publisher = PluginPublisher {
        id: publisher.publisher_id.clone(),
        name: publisher.name.clone(),
        website: publisher.website.clone(),
        verified: true,
    };
    let updates_existing_catalog = existing_catalog.is_some();
    let catalog = match existing_catalog {
        Some(catalog) => catalog,
        None => {
            publish_plugin_catalog_entry(
                &state,
                &user,
                PluginCatalogPayload {
                    marketplace_id: marketplace.id.clone(),
                    name: stored.normalized_manifest.name.clone(),
                    display_name: stored.normalized_manifest.interface.display_name.clone(),
                    description: stored.normalized_manifest.description.clone(),
                    publisher: plugin_publisher.clone(),
                    interface: stored.normalized_manifest.interface.clone(),
                    keywords: stored.normalized_manifest.keywords.clone(),
                    visibility: request.visibility.clone(),
                    featured: request.featured,
                    enabled: true,
                    has_ui: !stored.normalized_manifest.ui.is_empty(),
                    license: PluginLicenseMetadata {
                        license_id: request.license_id.clone(),
                        license_url: license_url.clone(),
                        redistributable: request.redistributable,
                        reviewed_at: request.redistributable.then(now_rfc3339),
                    },
                },
            )
            .await?
        }
    };

    let signed_at = now_rfc3339();
    let manifest_sha256 = normalized_plugin_manifest_sha256(&stored.normalized_manifest)
        .map_err(|error| ApiError::internal(format!("hash normalized Manifest failed: {error}")))?;
    let mut signature = PluginReleaseSignature {
        key_id: managed_key.key_id,
        publisher_id: publisher.publisher_id,
        marketplace_id: marketplace.id,
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        signature_base64: String::new(),
        signed_at,
        manifest_sha256,
    };
    let signing_payload = plugin_release_signing_payload(
        PluginReleaseVerificationContext {
            plugin_id: catalog.id.as_str(),
            version: stored.normalized_manifest.version.as_str(),
            marketplace_id: signature.marketplace_id.as_str(),
            publisher_id: signature.publisher_id.as_str(),
            artifact_sha256: stored.artifact_sha256.as_str(),
        },
        &signature,
    )
    .map_err(|error| ApiError::internal(format!("build Release signature failed: {error}")))?;
    signature.signature_base64 =
        STANDARD.encode(key_pair.sign(signing_payload.as_slice()).as_ref());

    let release = publish_plugin_release_from_manifest(
        &state,
        &user,
        catalog.id.as_str(),
        PluginReleasePayload {
            manifest: serde_json::to_value(&stored.normalized_manifest).map_err(|error| {
                ApiError::internal(format!("serialize normalized Manifest failed: {error}"))
            })?,
            version: Some(stored.normalized_manifest.version.clone()),
            npm_package: stored.npm_package,
            artifact_ref: stored.artifact_ref,
            artifact_sha256: stored.artifact_sha256,
            signature,
            sbom_ref: None,
            release_channel: request.release_channel,
        },
        stored.normalized_manifest.clone(),
        stored.skill_snapshots.clone(),
    )
    .await?;
    let mut catalog = state
        .store
        .get_plugin_catalog_entry(catalog.id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::internal("published Plugin catalog entry is missing"))?;
    if updates_existing_catalog {
        // Package publishing refreshes presentation data only. Governance fields
        // (visibility, featured status and reviewed license metadata) belong to
        // the catalog and must not be reset by release-form defaults.
        apply_uploaded_presentation_metadata(
            &mut catalog,
            &stored.normalized_manifest,
            plugin_publisher,
        );
        catalog.updated_at = now_rfc3339();
        state
            .store
            .replace_plugin_catalog_entry(&catalog)
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(Json(PublishUploadedPluginResponse { catalog, release }))
}

fn apply_uploaded_presentation_metadata(
    catalog: &mut PluginCatalogRecord,
    manifest: &PluginManifest,
    publisher: PluginPublisher,
) {
    catalog.display_name = manifest.interface.display_name.clone();
    catalog.description = manifest.description.clone();
    catalog.publisher = publisher;
    catalog.interface = manifest.interface.clone();
    catalog.keywords = manifest.keywords.clone();
    catalog.has_ui = !manifest.ui.is_empty();
}

pub(super) async fn download_plugin_artifact(
    State(state): State<AppState>,
    Path(artifact_sha256): Path<String>,
) -> Result<Response, ApiError> {
    let artifact_sha256 = normalize_sha256(artifact_sha256.as_str(), "artifact_sha256")?;
    let path = artifact_package_path(&state, artifact_sha256.as_str());
    let bytes = tokio::fs::read(path.as_path())
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => ApiError::not_found("Plugin artifact not found"),
            _ => ApiError::internal(format!("read Plugin artifact failed: {error}")),
        })?;
    if bytes.len() > state.config.plugin_artifact_max_bytes {
        return Err(ApiError::internal(
            "stored Plugin artifact exceeds its configured limit",
        ));
    }
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(format!("attachment; filename=\"{artifact_sha256}.tgz\"").as_str())
            .map_err(|error| ApiError::internal(format!("build artifact header failed: {error}")))?,
    );
    Ok(response)
}

pub(super) async fn download_plugin_artifact_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(artifact_sha256): Path<String>,
) -> Result<Response, ApiError> {
    let identity =
        require_local_connector_internal_request(&state, &headers, PLUGIN_INSTALL_MANAGE_SCOPE)?;
    let mut audit = PluginManagementInternalAuditGuard::new(
        &identity,
        None,
        "plugin_artifact",
        artifact_sha256.as_str(),
        "download",
    );
    audit.resource_name(Some(artifact_sha256.as_str()));
    let response = download_plugin_artifact(State(state), Path(artifact_sha256)).await?;
    audit.succeeded();
    Ok(response)
}

fn persist_and_analyze_package(
    state: &AppState,
    package_bytes: Vec<u8>,
    manifest_override: Option<String>,
) -> Result<PluginPackageAnalysis, ApiError> {
    let parsed = parse_npm_package(package_bytes.as_slice(), manifest_override.as_deref())?;
    let artifact_sha256 = hex::encode(Sha256::digest(package_bytes.as_slice()));
    let npm_integrity = format!(
        "sha512-{}",
        STANDARD.encode(Sha512::digest(package_bytes.as_slice()))
    );
    let artifact_ref = format!(
        "{}/api/plugin-artifacts/{}",
        state.config.plugin_artifact_public_base_url, artifact_sha256
    );
    let npm_package = PluginNpmPackage {
        name: parsed.package_name.clone(),
        version: parsed.package_version.clone(),
        integrity: npm_integrity.clone(),
    };
    let stored = StoredPluginArtifactMetadata {
        artifact_sha256: artifact_sha256.clone(),
        artifact_ref: artifact_ref.clone(),
        npm_package,
        normalized_manifest: parsed.manifest.clone(),
        skill_snapshots: parsed.skill_snapshots.clone(),
    };
    write_artifact_atomically(
        artifact_package_path(state, artifact_sha256.as_str()).as_path(),
        package_bytes.as_slice(),
    )?;
    let metadata = serde_json::to_vec_pretty(&stored).map_err(|error| {
        ApiError::internal(format!("serialize artifact metadata failed: {error}"))
    })?;
    write_artifact_atomically(
        artifact_metadata_path(state, artifact_sha256.as_str()).as_path(),
        metadata.as_slice(),
    )?;
    Ok(PluginPackageAnalysis {
        artifact_sha256,
        artifact_ref,
        package_name: parsed.package_name,
        package_version: parsed.package_version,
        npm_integrity,
        package_bins: parsed.package_bins,
        has_ui: !parsed.manifest.ui.is_empty(),
        components: plugin_component_descriptors(&parsed.manifest),
        skill_snapshots: parsed.skill_snapshots,
        manifest: parsed.manifest,
    })
}

fn parse_npm_package(
    bytes: &[u8],
    manifest_override: Option<&str>,
) -> Result<ParsedPackage, ApiError> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let mut package_json = None;
    let mut packaged_manifests = vec![None; MANIFEST_PATHS.len()];
    let mut archived_files = BTreeSet::new();
    let mut entry_count = 0usize;
    let mut uncompressed_bytes = 0u64;
    for entry in archive.entries().map_err(|error| {
        ApiError::bad_request(format!("read npm package archive failed: {error}"))
    })? {
        let mut entry = entry.map_err(|error| {
            ApiError::bad_request(format!("read npm package entry failed: {error}"))
        })?;
        entry_count += 1;
        if entry_count > MAX_ARCHIVE_ENTRIES {
            return Err(ApiError::bad_request(
                "npm package contains too many archive entries",
            ));
        }
        let size = entry.size();
        uncompressed_bytes = uncompressed_bytes.saturating_add(size);
        if uncompressed_bytes > MAX_UNCOMPRESSED_BYTES {
            return Err(ApiError::bad_request(
                "npm package uncompressed size exceeds its limit",
            ));
        }
        let path = entry.path().map_err(|error| {
            ApiError::bad_request(format!("read npm package path failed: {error}"))
        })?;
        validate_archive_path(path.as_ref())?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink()
            || entry_type.is_hard_link()
            || entry_type.is_block_special()
            || entry_type.is_character_special()
            || entry_type.is_fifo()
        {
            return Err(ApiError::bad_request(
                "npm package contains a forbidden archive entry",
            ));
        }
        let path_text = path.to_string_lossy();
        if entry_type.is_file() {
            archived_files.insert(path_text.to_string());
        } else if !entry_type.is_dir() {
            return Err(ApiError::bad_request(
                "npm package contains an unsupported archive entry",
            ));
        }
        let wants_package = path_text == PACKAGE_JSON_PATH;
        let manifest_index = MANIFEST_PATHS
            .iter()
            .position(|candidate| *candidate == path_text);
        if !wants_package && manifest_index.is_none() {
            continue;
        }
        if size > MAX_PACKAGE_METADATA_BYTES {
            return Err(ApiError::bad_request(
                "npm package metadata file exceeds its size limit",
            ));
        }
        let mut content = Vec::with_capacity(size as usize);
        entry.read_to_end(&mut content).map_err(|error| {
            ApiError::bad_request(format!("read npm package metadata failed: {error}"))
        })?;
        if wants_package {
            if package_json.replace(content).is_some() {
                return Err(ApiError::bad_request(
                    "npm package contains duplicate package.json",
                ));
            }
        } else if let Some(index) = manifest_index {
            if packaged_manifests[index].replace(content).is_some() {
                return Err(ApiError::bad_request(
                    "npm package contains a duplicate Plugin Manifest",
                ));
            }
        }
    }
    let package_json = package_json
        .ok_or_else(|| ApiError::bad_request("npm package is missing package/package.json"))?;
    let package: NpmPackageJson = serde_json::from_slice(package_json.as_slice())
        .map_err(|error| ApiError::bad_request(format!("package.json is invalid: {error}")))?;
    let manifest_json = manifest_override
        .map(str::as_bytes)
        .or_else(|| packaged_manifests.iter().find_map(Option::as_deref))
        .ok_or_else(|| {
            ApiError::bad_request(
                "Plugin Manifest is required as an upload or package/chatos.plugin.json",
            )
        })?;
    let manifest_text = std::str::from_utf8(manifest_json)
        .map_err(|_| ApiError::bad_request("Plugin Manifest must use UTF-8 JSON"))?;
    let manifest = parse_plugin_manifest(manifest_text)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    if package.name.trim().is_empty() || package.version.trim().is_empty() {
        return Err(ApiError::bad_request(
            "package.json name and version are required",
        ));
    }
    if package.version.trim() != manifest.version {
        return Err(ApiError::bad_request(
            "package.json version must match Plugin Manifest version",
        ));
    }
    let package_bins = package_bins(&package.bin, package.name.as_str())?;
    let mut required_bins = BTreeMap::new();
    for server in &manifest.mcp_servers {
        if let PluginMcpServer::Stdio { bin, .. } = server {
            required_bins.insert(bin.as_str(), "stdio");
        }
    }
    for ui in &manifest.ui {
        if let Some(PluginUiRuntime::LocalHttp { bin, .. }) = &ui.runtime {
            required_bins.insert(bin.as_str(), "UI runtime");
        }
    }
    for (bin, usage) in required_bins {
        let package_bin = package_bins
            .iter()
            .find(|candidate| candidate.name == bin)
            .ok_or_else(|| {
                ApiError::bad_request(format!(
                    "Plugin Manifest {usage} bin {bin} is not declared by package.json.bin"
                ))
            })?;
        if !archived_files.contains(package_bin.archive_path.as_str()) {
            return Err(ApiError::bad_request(format!(
                "package.json.bin entry {bin} points to a file missing from the npm package"
            )));
        }
    }
    Ok(ParsedPackage {
        package_name: package.name.trim().to_string(),
        package_version: package.version.trim().to_string(),
        package_bins: package_bins.into_iter().map(|item| item.name).collect(),
        skill_snapshots: analyze_packaged_skills(bytes, &manifest)?,
        manifest,
    })
}

#[derive(Debug)]
struct PackagedSkillFiles {
    collection_path: String,
    skill_document: Option<Vec<u8>>,
    resources: BTreeMap<String, Vec<u8>>,
    total_resource_bytes: u64,
}

include!("plugin_package_publish_part01.rs");
include!("plugin_package_publish_part02.rs");
