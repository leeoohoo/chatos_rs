// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_plugin_management_sdk::{
    parse_plugin_manifest, plugin_component_descriptors, verify_plugin_release_signature,
    PluginReleaseVerificationContext,
};
use semver::Version;

use super::plugin_publishers::require_approved_publisher_release_key;
use super::*;

pub(super) async fn list_plugin_releases(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
) -> Result<Json<ListResponse<PluginReleaseRecord>>, ApiError> {
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    ensure_catalog_visible(&user, &plugin)?;
    let items = state
        .store
        .list_plugin_releases(plugin.id.as_str(), user.is_super_admin())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn create_plugin_release(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
    Json(mut payload): Json<PluginReleasePayload>,
) -> Result<Json<PluginReleaseRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let mut plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    let marketplace = state
        .store
        .get_plugin_marketplace(plugin.marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::conflict("Plugin marketplace not found"))?;
    if !marketplace.enabled || marketplace.trust_level == PLUGIN_TRUST_UNTRUSTED {
        return Err(ApiError::conflict(
            "Plugin marketplace is not trusted for release publishing",
        ));
    }
    require_approved_publisher_release_key(
        &state,
        &marketplace,
        &plugin.publisher,
        payload.signature.key_id.as_str(),
    )
    .await?;

    let manifest_json = serde_json::to_string(&payload.manifest)
        .map_err(|err| ApiError::bad_request(format!("serialize manifest failed: {err}")))?;
    let manifest = parse_plugin_manifest(manifest_json.as_str())
        .map_err(|err| ApiError::bad_request(err.to_string()))?;
    validate_release_manifest_identity(&plugin, payload.version.as_deref(), &manifest)?;
    validate_npm_package(&payload.npm_package, &manifest)?;
    payload.artifact_sha256 =
        normalize_sha256(payload.artifact_sha256.as_str(), "artifact_sha256")?;
    payload.signature.manifest_sha256 = normalize_sha256(
        payload.signature.manifest_sha256.as_str(),
        "signature.manifest_sha256",
    )?;
    validate_release_signature(
        &plugin,
        &marketplace,
        &manifest,
        payload.artifact_sha256.as_str(),
        &payload.signature,
    )?;
    let release_channel = normalize_release_channel(payload.release_channel.as_str())?;
    validate_stable_release_progression(
        &state,
        &plugin,
        manifest.version.as_str(),
        &release_channel,
    )
    .await?;
    if state
        .store
        .find_plugin_release_by_version(plugin.id.as_str(), manifest.version.as_str())
        .await
        .map_err(ApiError::internal)?
        .is_some()
    {
        return Err(ApiError::conflict(
            "Plugin release version is immutable and already exists",
        ));
    }

    let components = plugin_component_descriptors(&manifest);
    let release = PluginReleaseRecord {
        id: Uuid::new_v4().to_string(),
        plugin_id: plugin.id.clone(),
        version: manifest.version.clone(),
        manifest_schema_version: manifest.schema_version,
        normalized_manifest: manifest.clone(),
        npm_package: payload.npm_package,
        artifact_ref: required_text(Some(payload.artifact_ref.as_str()), "artifact_ref")?,
        artifact_sha256: payload.artifact_sha256,
        signature: payload.signature,
        sbom_ref: payload
            .sbom_ref
            .as_deref()
            .and_then(|value| normalized(Some(value))),
        supported_platforms: manifest.dependencies.supported_platforms.clone(),
        components,
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        release_channel: release_channel.clone(),
        published_at: now_rfc3339(),
        revoked_at: None,
    };
    let component_snapshots = release
        .components
        .iter()
        .cloned()
        .map(|component| PluginComponentSnapshot {
            plugin_id: release.plugin_id.clone(),
            release_id: release.id.clone(),
            component,
            content_sha256: release.artifact_sha256.clone(),
        })
        .collect::<Vec<_>>();
    state
        .store
        .set_plugin_release_publication_ready(release.id.as_str(), false)
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .insert_plugin_release(&release)
        .await
        .map_err(|err| {
            if err.contains("E11000") {
                ApiError::conflict("Plugin release version is immutable and already exists")
            } else {
                ApiError::internal(err)
            }
        })?;
    state
        .store
        .replace_plugin_component_snapshots(
            release.plugin_id.as_str(),
            release.id.as_str(),
            component_snapshots.as_slice(),
        )
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .set_plugin_release_publication_ready(release.id.as_str(), true)
        .await
        .map_err(ApiError::internal)?;
    if release_channel == "stable" {
        plugin.latest_release_id = release.id.clone();
        plugin.updated_at = now_rfc3339();
        state
            .store
            .replace_plugin_catalog_entry(&plugin)
            .await
            .map_err(ApiError::internal)?;
    }
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_PUBLISH_RELEASE,
        user.user_id.as_str(),
        None,
        plugin.id.as_str(),
        Some(release.id.as_str()),
        "success",
        BTreeMap::from([
            ("version".to_string(), json!(release.version)),
            (
                "release_channel".to_string(),
                json!(release.release_channel),
            ),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(release))
}

pub(super) async fn revoke_plugin_release(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(release_id): Path<String>,
) -> Result<Json<PluginReleaseRecord>, ApiError> {
    ensure_super_admin(&user)?;
    let mut release = state
        .store
        .get_plugin_release(release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin release not found"))?;
    if release.revoked_at.is_some() {
        return Err(ApiError::conflict("Plugin release is already revoked"));
    }
    release.revoked_at = Some(now_rfc3339());
    state
        .store
        .replace_plugin_release(&release)
        .await
        .map_err(ApiError::internal)?;

    let mut plugin = state
        .store
        .get_plugin_catalog_entry(release.plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    if plugin.latest_release_id == release.id {
        let releases = state
            .store
            .list_plugin_releases(plugin.id.as_str(), false)
            .await
            .map_err(ApiError::internal)?;
        plugin.latest_release_id = latest_stable_release(&releases)
            .map(|item| item.id.clone())
            .unwrap_or_default();
        plugin.updated_at = now_rfc3339();
        state
            .store
            .replace_plugin_catalog_entry(&plugin)
            .await
            .map_err(ApiError::internal)?;
    }
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_REVOKE_RELEASE,
        user.user_id.as_str(),
        None,
        plugin.id.as_str(),
        Some(release.id.as_str()),
        "success",
        BTreeMap::from([("version".to_string(), json!(release.version))]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(release))
}

fn validate_release_manifest_identity(
    plugin: &PluginCatalogRecord,
    declared_version: Option<&str>,
    manifest: &PluginManifest,
) -> Result<(), ApiError> {
    if manifest.name != plugin.name {
        return Err(ApiError::bad_request(
            "manifest name does not match Plugin catalog identity",
        ));
    }
    if let Some(version) = declared_version {
        if version.trim() != manifest.version {
            return Err(ApiError::bad_request(
                "payload version does not match manifest version",
            ));
        }
    }
    Ok(())
}

fn validate_npm_package(
    package: &PluginNpmPackage,
    manifest: &PluginManifest,
) -> Result<(), ApiError> {
    let name = package.name.trim();
    let valid_name = !name.is_empty()
        && name.len() <= 214
        && !name.contains(char::is_whitespace)
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'/' | b'-' | b'_' | b'.')
        });
    if !valid_name || name.starts_with('/') || name.ends_with('/') || name.matches('/').count() > 1
    {
        return Err(ApiError::bad_request("npm_package.name is invalid"));
    }
    if package.version.trim() != manifest.version {
        return Err(ApiError::bad_request(
            "npm_package.version must match the Plugin manifest version",
        ));
    }
    let integrity = package.integrity.trim();
    if !integrity.starts_with("sha512-") || integrity.len() <= "sha512-".len() {
        return Err(ApiError::bad_request(
            "npm_package.integrity must be an npm sha512 integrity value",
        ));
    }
    Ok(())
}

fn validate_release_signature(
    plugin: &PluginCatalogRecord,
    marketplace: &PluginMarketplaceRecord,
    manifest: &PluginManifest,
    artifact_sha256: &str,
    signature: &PluginReleaseSignature,
) -> Result<(), ApiError> {
    let key = marketplace
        .trusted_signing_keys
        .iter()
        .find(|key| key.key_id == signature.key_id)
        .ok_or_else(|| ApiError::bad_request("release signature key is not trusted"))?;
    verify_plugin_release_signature(
        PluginReleaseVerificationContext {
            plugin_id: plugin.id.as_str(),
            version: manifest.version.as_str(),
            marketplace_id: marketplace.id.as_str(),
            publisher_id: plugin.publisher.id.as_str(),
            artifact_sha256,
        },
        manifest,
        signature,
        key,
    )
    .map_err(|err| ApiError::bad_request(format!("release signature verification failed: {err}")))
}

async fn validate_stable_release_progression(
    state: &AppState,
    plugin: &PluginCatalogRecord,
    version: &str,
    release_channel: &str,
) -> Result<(), ApiError> {
    if release_channel != "stable" || plugin.latest_release_id.is_empty() {
        return Ok(());
    }
    let latest = state
        .store
        .get_plugin_release(plugin.latest_release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::conflict("latest Plugin release is missing"))?;
    let next = Version::parse(version)
        .map_err(|_| ApiError::bad_request("manifest version must use strict semver"))?;
    let current = Version::parse(latest.version.as_str())
        .map_err(|_| ApiError::conflict("latest Plugin release has invalid semver"))?;
    if next <= current {
        return Err(ApiError::conflict(
            "stable Plugin release must be newer than the active catalog release",
        ));
    }
    Ok(())
}

fn latest_stable_release(releases: &[PluginReleaseRecord]) -> Option<&PluginReleaseRecord> {
    releases
        .iter()
        .filter(|release| release.release_channel == "stable" && release.revoked_at.is_none())
        .filter_map(|release| {
            Version::parse(release.version.as_str())
                .ok()
                .map(|v| (v, release))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, release)| release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_stable_release_ignores_revoked_and_prerelease_channels() {
        let stable = release("2.0.0", "stable");
        let beta = release("3.0.0", "beta");
        let mut revoked = release("4.0.0", "stable");
        revoked.revoked_at = Some("now".to_string());
        let items = vec![beta, revoked, stable.clone()];
        assert_eq!(
            latest_stable_release(&items).map(|item| item.version.as_str()),
            Some("2.0.0")
        );
    }

    fn release(version: &str, channel: &str) -> PluginReleaseRecord {
        serde_json::from_value(json!({
            "id": version,
            "plugin_id": "plugin-1",
            "version": version,
            "manifest_schema_version": 3,
            "normalized_manifest": {
                "schemaVersion": 3,
                "name": "demo",
                "version": version,
                "description": "demo",
                "author": {"name": "ChatOS"},
                "keywords": [],
                "skills": ["./skills"],
                "mcpServers": [],
                "apps": [],
                "commands": [],
                "agents": [],
                "hooks": [],
                "ui": [],
                "interface": {
                    "displayName": "Demo",
                    "shortDescription": "Demo",
                    "longDescription": "Demo plugin",
                    "developerName": "ChatOS",
                    "category": "Developer Tools",
                    "capabilities": [],
                    "defaultPrompt": [],
                    "screenshots": []
                },
                "dependencies": {"plugins": [], "executables": [], "supportedPlatforms": []},
                "permissions": []
            },
            "npm_package": {
                "name": "demo",
                "version": version,
                "integrity": "sha512-dGVzdA=="
            },
            "artifact_ref": "artifact",
            "artifact_sha256": "a".repeat(64),
            "signature": {
                "key_id": "key",
                "publisher_id": "publisher",
                "marketplace_id": "marketplace",
                "algorithm": "ed25519",
                "signature_base64": "signature",
                "signed_at": "now",
                "manifest_sha256": "b".repeat(64)
            },
            "supported_platforms": [],
            "components": [],
            "dependencies": {"plugins": [], "executables": [], "supportedPlatforms": []},
            "permissions": [],
            "release_channel": channel,
            "published_at": "now",
            "revoked_at": null
        }))
        .expect("release fixture")
    }
}
