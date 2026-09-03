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
    normalized_plugin_manifest_sha256, parse_plugin_manifest, plugin_component_descriptors,
    plugin_release_signing_payload, PluginLicenseMetadata, PluginManifest, PluginMcpServer,
    PluginNpmPackage, PluginPublisher, PluginReleaseSignature, PluginReleaseVerificationContext,
    PluginUiRuntime, SigningKeyRef, PLUGIN_SIGNATURE_ALGORITHM_ED25519,
    PLUGIN_SIGNING_KEY_USAGE_RELEASE,
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

#[derive(Debug, Serialize, Deserialize)]
struct StoredPluginArtifactMetadata {
    artifact_sha256: String,
    artifact_ref: String,
    npm_package: PluginNpmPackage,
    normalized_manifest: PluginManifest,
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
    let analysis = persist_and_analyze_package(&state, package_bytes, manifest_override)?;
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

    let stored = read_stored_artifact_metadata(&state, request.artifact_sha256.as_str())?;
    verify_stored_artifact(&state, &stored)?;
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
        manifest,
    })
}

fn package_bins(value: &Value, package_name: &str) -> Result<Vec<PackageBin>, ApiError> {
    let values = match value {
        Value::String(target) => BTreeMap::from([(
            package_name
                .rsplit('/')
                .next()
                .unwrap_or(package_name)
                .to_string(),
            target.as_str(),
        )]),
        Value::Object(values) => values
            .iter()
            .map(|(name, target)| {
                target
                    .as_str()
                    .map(|target| (name.clone(), target))
                    .ok_or_else(|| ApiError::bad_request("package.json.bin values must be strings"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?,
        Value::Null => BTreeMap::new(),
        _ => {
            return Err(ApiError::bad_request(
                "package.json.bin must be a string or object",
            ))
        }
    };
    values
        .into_iter()
        .map(|(name, target)| {
            let name = name.trim();
            if name.is_empty() || name.contains('/') || name.contains('\\') {
                return Err(ApiError::bad_request(
                    "package.json.bin names must be non-empty executable names",
                ));
            }
            let target = target.trim().strip_prefix("./").unwrap_or(target.trim());
            let target_path = FsPath::new(target);
            if target.is_empty()
                || target_path.is_absolute()
                || target_path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(ApiError::bad_request(
                    "package.json.bin targets must be safe relative package paths",
                ));
            }
            Ok(PackageBin {
                name: name.to_string(),
                archive_path: format!("package/{target}"),
            })
        })
        .collect()
}

fn validate_archive_path(path: &FsPath) -> Result<(), ApiError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            != Some("package")
    {
        return Err(ApiError::bad_request(
            "npm package contains an unsafe archive path",
        ));
    }
    Ok(())
}

async fn ensure_managed_release_key(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    publisher: &PluginPublisherRecord,
) -> Result<(SigningKeyRef, Ed25519KeyPair), ApiError> {
    let (key_ref, key_pair) = load_or_create_managed_key(state, marketplace, publisher)?;
    if !publisher
        .signing_keys
        .iter()
        .any(|key| key.key_id == key_ref.key_id)
    {
        let mut updated = publisher.clone();
        updated.signing_keys.push(key_ref.clone());
        updated
            .signing_keys
            .sort_by(|left, right| left.key_id.cmp(&right.key_id));
        updated.updated_at = now_rfc3339();
        if !state
            .store
            .replace_plugin_publisher_if_matches(publisher, &updated)
            .await
            .map_err(ApiError::internal)?
        {
            return Err(ApiError::conflict(
                "Plugin publisher changed concurrently; retry publishing",
            ));
        }
    }
    if !marketplace
        .trusted_signing_keys
        .iter()
        .any(|key| key.key_id == key_ref.key_id)
    {
        let mut updated = marketplace.clone();
        updated.trusted_signing_keys.push(key_ref.clone());
        updated
            .trusted_signing_keys
            .sort_by(|left, right| left.key_id.cmp(&right.key_id));
        if !state
            .store
            .replace_plugin_marketplace_if_matches_with_catalog_sync(
                marketplace,
                &updated,
                is_syncable_network_marketplace(&updated),
            )
            .await
            .map_err(ApiError::internal)?
        {
            return Err(ApiError::conflict(
                "Plugin marketplace changed concurrently; retry publishing",
            ));
        }
    }
    Ok((key_ref, key_pair))
}

fn load_or_create_managed_key(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    publisher: &PluginPublisherRecord,
) -> Result<(SigningKeyRef, Ed25519KeyPair), ApiError> {
    let key_dir = state
        .config
        .plugin_artifact_storage_dir
        .join("managed-signing");
    fs::create_dir_all(key_dir.as_path()).map_err(|error| {
        ApiError::internal(format!("create managed signer directory failed: {error}"))
    })?;
    restrict_directory_permissions(key_dir.as_path())?;
    let scope_hash = hex::encode(Sha256::digest(
        format!("{}\0{}", marketplace.id, publisher.publisher_id).as_bytes(),
    ));
    let path = key_dir.join(format!("{}.pk8", &scope_hash[..32]));
    let key_bytes = if path.exists() {
        Zeroizing::new(fs::read(path.as_path()).map_err(|error| {
            ApiError::internal(format!("read managed Release signing key failed: {error}"))
        })?)
    } else {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| ApiError::internal("generate managed Release signing key failed"))?;
        write_artifact_atomically(path.as_path(), document.as_ref())?;
        restrict_file_permissions(path.as_path())?;
        Zeroizing::new(document.as_ref().to_vec())
    };
    let key_pair = Ed25519KeyPair::from_pkcs8(key_bytes.as_slice())
        .map_err(|_| ApiError::internal("managed Release signing key is invalid"))?;
    let public_key_base64 = STANDARD.encode(key_pair.public_key().as_ref());
    let key_id = format!(
        "managed-{}",
        &hex::encode(Sha256::digest(key_pair.public_key().as_ref()))[..24]
    );
    let existing = publisher
        .signing_keys
        .iter()
        .find(|key| key.key_id == key_id);
    let key_ref = existing.cloned().unwrap_or_else(|| SigningKeyRef {
        key_id,
        publisher_id: publisher.publisher_id.clone(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        public_key_base64,
        usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
        valid_from: now_rfc3339(),
        valid_until: None,
        revoked_at: None,
    });
    Ok((key_ref, key_pair))
}

fn read_stored_artifact_metadata(
    state: &AppState,
    artifact_sha256: &str,
) -> Result<StoredPluginArtifactMetadata, ApiError> {
    let bytes = fs::read(artifact_metadata_path(state, artifact_sha256)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ApiError::not_found("uploaded Plugin artifact metadata not found")
        } else {
            ApiError::internal(format!("read Plugin artifact metadata failed: {error}"))
        }
    })?;
    serde_json::from_slice(bytes.as_slice()).map_err(|error| {
        ApiError::internal(format!("decode Plugin artifact metadata failed: {error}"))
    })
}

fn verify_stored_artifact(
    state: &AppState,
    stored: &StoredPluginArtifactMetadata,
) -> Result<(), ApiError> {
    let bytes = fs::read(artifact_package_path(
        state,
        stored.artifact_sha256.as_str(),
    ))
    .map_err(|error| {
        ApiError::internal(format!("read uploaded Plugin artifact failed: {error}"))
    })?;
    if bytes.len() > state.config.plugin_artifact_max_bytes
        || hex::encode(Sha256::digest(bytes.as_slice())) != stored.artifact_sha256
        || format!(
            "sha512-{}",
            STANDARD.encode(Sha512::digest(bytes.as_slice()))
        ) != stored.npm_package.integrity
    {
        return Err(ApiError::conflict(
            "uploaded Plugin artifact integrity has changed",
        ));
    }
    Ok(())
}

fn artifact_package_path(state: &AppState, artifact_sha256: &str) -> PathBuf {
    state
        .config
        .plugin_artifact_storage_dir
        .join(format!("{artifact_sha256}.tgz"))
}

fn artifact_metadata_path(state: &AppState, artifact_sha256: &str) -> PathBuf {
    state
        .config
        .plugin_artifact_storage_dir
        .join(format!("{artifact_sha256}.json"))
}

fn write_artifact_atomically(path: &FsPath, bytes: &[u8]) -> Result<(), ApiError> {
    if path.exists() {
        let existing = fs::read(path).map_err(|error| {
            ApiError::internal(format!("read immutable Plugin artifact failed: {error}"))
        })?;
        return if existing == bytes {
            Ok(())
        } else {
            Err(ApiError::conflict(
                "immutable Plugin artifact path already contains different content",
            ))
        };
    }
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::internal("Plugin artifact path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        ApiError::internal(format!("create Plugin artifact directory failed: {error}"))
    })?;
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    fs::write(temporary.as_path(), bytes)
        .map_err(|error| ApiError::internal(format!("write Plugin artifact failed: {error}")))?;
    restrict_file_permissions(temporary.as_path())?;
    match fs::rename(temporary.as_path(), path) {
        Ok(()) => Ok(()),
        Err(_error) if path.exists() => {
            let existing = fs::read(path).map_err(|error| {
                ApiError::internal(format!(
                    "read concurrently committed Plugin artifact failed: {error}"
                ))
            })?;
            let _ = fs::remove_file(temporary);
            if existing == bytes {
                Ok(())
            } else {
                Err(ApiError::conflict(
                    "immutable Plugin artifact path was concurrently committed with different content",
                ))
            }
        }
        Err(error) => {
            let _ = fs::remove_file(temporary);
            Err(ApiError::internal(format!(
                "commit Plugin artifact failed: {error}"
            )))
        }
    }
}

fn normalize_optional_https_url(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let url = reqwest::Url::parse(value)
        .map_err(|_| ApiError::bad_request("license_url is not a valid URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::bad_request(
            "license_url must be a plain HTTPS URL",
        ));
    }
    Ok(Some(value.to_string()))
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &FsPath) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| ApiError::internal(format!("protect signer directory failed: {error}")))
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &FsPath) -> Result<(), ApiError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(path: &FsPath) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        ApiError::internal(format!("protect Plugin artifact file failed: {error}"))
    })
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &FsPath) -> Result<(), ApiError> {
    Ok(())
}

fn default_public_visibility() -> String {
    PLUGIN_VISIBILITY_PUBLIC.to_string()
}

fn default_stable_channel() -> String {
    "stable".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn package_fixture(manifest: &str) -> Vec<u8> {
        package_fixture_with_bin_file(manifest, true)
    }

    fn package_fixture_with_bin_file(manifest: &str, include_bin_file: bool) -> Vec<u8> {
        let mut encoded = Vec::new();
        {
            let gzip = GzEncoder::new(&mut encoded, Compression::default());
            let mut archive = tar::Builder::new(gzip);
            append(
                &mut archive,
                PACKAGE_JSON_PATH,
                br#"{"name":"demo-mcp","version":"1.0.0","bin":{"demo-mcp":"dist/cli.js"}}"#,
            );
            append(&mut archive, MANIFEST_PATHS[0], manifest.as_bytes());
            if include_bin_file {
                append(
                    &mut archive,
                    "package/dist/cli.js",
                    b"#!/usr/bin/env node\n",
                );
            }
            archive
                .into_inner()
                .expect("gzip")
                .finish()
                .expect("finish");
        }
        encoded
    }

    fn append<W: Write>(archive: &mut tar::Builder<W>, path: &str, bytes: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, bytes)
            .expect("append");
    }

    #[test]
    fn uploaded_package_parses_manifest_and_requires_declared_stdio_bin() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let parsed = parse_npm_package(package_fixture(manifest).as_slice(), None).expect("parse");
        assert_eq!(parsed.package_name, "demo-mcp");
        assert_eq!(parsed.package_bins, vec!["demo-mcp"]);
        assert_eq!(parsed.manifest.mcp_servers.len(), 1);
    }

    #[test]
    fn uploaded_package_rejects_stdio_bin_missing_from_package_json() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "mcpServers":{"demo":{"type":"stdio","bin":"missing-bin"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let error = parse_npm_package(package_fixture(manifest).as_slice(), None).unwrap_err();
        assert!(error.message.contains("not declared"));
    }

    #[test]
    fn uploaded_package_rejects_declared_bin_file_missing_from_archive() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let error = parse_npm_package(
            package_fixture_with_bin_file(manifest, false).as_slice(),
            None,
        )
        .unwrap_err();
        assert!(error.message.contains("missing from the npm package"));
    }

    #[test]
    fn uploaded_package_accepts_declared_local_ui_runtime_bin() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo UI",
          "author":{"name":"Demo"},
          "ui":[{"componentKey":"workbench","source":"./ui/index.html","surface":"workbench","runtime":{"type":"local_http","bin":"demo-mcp","args":["studio"]}}],
          "interface":{"displayName":"Demo UI","shortDescription":"Demo","longDescription":"Demo UI","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start UI","components":["workbench"]}]
        }"#;
        let parsed = parse_npm_package(package_fixture(manifest).as_slice(), None).expect("parse");
        assert_eq!(parsed.package_bins, vec!["demo-mcp"]);
        assert_eq!(parsed.manifest.ui.len(), 1);
    }

    #[test]
    fn uploaded_package_rejects_local_ui_runtime_bin_missing_from_package_json() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo UI",
          "author":{"name":"Demo"},
          "ui":[{"componentKey":"workbench","source":"./ui/index.html","surface":"workbench","runtime":{"type":"local_http","bin":"missing-bin"}}],
          "interface":{"displayName":"Demo UI","shortDescription":"Demo","longDescription":"Demo UI","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start UI","components":["workbench"]}]
        }"#;
        let error = parse_npm_package(package_fixture(manifest).as_slice(), None).unwrap_err();
        assert!(error.message.contains("UI runtime bin missing-bin"));
        assert!(error.message.contains("not declared"));
    }

    #[test]
    fn uploaded_package_rejects_local_ui_runtime_bin_file_missing_from_archive() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo UI",
          "author":{"name":"Demo"},
          "ui":[{"componentKey":"workbench","source":"./ui/index.html","surface":"workbench","runtime":{"type":"local_http","bin":"demo-mcp"}}],
          "interface":{"displayName":"Demo UI","shortDescription":"Demo","longDescription":"Demo UI","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start UI","components":["workbench"]}]
        }"#;
        let error = parse_npm_package(
            package_fixture_with_bin_file(manifest, false).as_slice(),
            None,
        )
        .unwrap_err();
        assert!(error.message.contains("missing from the npm package"));
    }

    #[test]
    fn existing_release_refresh_preserves_catalog_governance_metadata() {
        let old_manifest = parse_plugin_manifest(r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Old description",
          "author":{"name":"Old Publisher"},
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}],
          "interface":{"displayName":"Old Name","shortDescription":"Old","longDescription":"Old description","developerName":"Old Publisher","category":"Developer Tools"}
        }"#).expect("old manifest");
        let new_manifest = parse_plugin_manifest(r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.1.0",
          "description":"New description",
          "author":{"name":"New Publisher"},
          "keywords":["browser","automation"],
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}],
          "interface":{"displayName":"New Name","shortDescription":"New","longDescription":"New description","developerName":"New Publisher","category":"Productivity"}
        }"#).expect("new manifest");
        let mut catalog = PluginCatalogRecord {
            id: "plugin-id".to_string(),
            plugin_key: "demo-mcp@marketplace".to_string(),
            marketplace_id: "marketplace".to_string(),
            owner_user_id: None,
            name: "demo-mcp".to_string(),
            display_name: old_manifest.interface.display_name.clone(),
            description: old_manifest.description.clone(),
            publisher: PluginPublisher {
                id: "publisher".to_string(),
                name: "Old Publisher".to_string(),
                website: None,
                verified: true,
            },
            interface: old_manifest.interface,
            keywords: Vec::new(),
            visibility: "private".to_string(),
            featured: true,
            enabled: true,
            has_ui: false,
            latest_release_id: "release-id".to_string(),
            license: PluginLicenseMetadata {
                license_id: "Apache-2.0".to_string(),
                license_url: Some("https://www.apache.org/licenses/LICENSE-2.0".to_string()),
                redistributable: true,
                reviewed_at: Some("2026-09-03T00:00:00Z".to_string()),
            },
            created_at: "2026-09-03T00:00:00Z".to_string(),
            updated_at: "2026-09-03T00:00:00Z".to_string(),
        };
        let governance_before = (
            catalog.visibility.clone(),
            catalog.featured,
            catalog.license.clone(),
            catalog.latest_release_id.clone(),
        );

        apply_uploaded_presentation_metadata(
            &mut catalog,
            &new_manifest,
            PluginPublisher {
                id: "publisher".to_string(),
                name: "New Publisher".to_string(),
                website: Some("https://example.com".to_string()),
                verified: true,
            },
        );

        assert_eq!(catalog.display_name, "New Name");
        assert_eq!(catalog.description, "New description");
        assert_eq!(catalog.publisher.name, "New Publisher");
        assert_eq!(catalog.keywords, vec!["automation", "browser"]);
        assert_eq!(
            (
                catalog.visibility,
                catalog.featured,
                catalog.license,
                catalog.latest_release_id,
            ),
            governance_before,
        );
    }
}
