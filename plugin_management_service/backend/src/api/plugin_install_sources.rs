// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::net::IpAddr;

use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, verify_plugin_release_signature, PluginInstallSource,
    PluginInstallSourceList, PluginReleaseVerificationContext,
};

use super::plugin_publishers::{
    require_approved_publisher_identity, require_approved_publisher_release_key,
};
use super::*;

pub(super) async fn list_plugin_install_sources_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PluginInstallSourceQuery>,
) -> Result<Json<PluginInstallSourceList>, ApiError> {
    require_local_connector_internal_request(&state, &headers, PLUGIN_INSTALL_MANAGE_SCOPE)?;
    let owner_user_id = required_text(Some(query.owner_user_id.as_str()), "owner_user_id")?;
    let catalog = state
        .store
        .list_plugin_catalog(
            &PluginCatalogQuery {
                enabled: Some(true),
                limit: Some(500),
                ..PluginCatalogQuery::default()
            },
            Some(owner_user_id.as_str()),
        )
        .await
        .map_err(ApiError::internal)?;
    let mut items = Vec::new();
    for plugin in catalog.items {
        if plugin.latest_release_id.trim().is_empty() {
            continue;
        }
        let marketplace = state
            .store
            .get_plugin_marketplace(plugin.marketplace_id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::conflict("Plugin Marketplace not found"))?;
        if !is_network_marketplace(&marketplace) {
            continue;
        }
        items.push(load_install_source(&state, plugin, None, owner_user_id.as_str()).await?);
    }
    Ok(Json(PluginInstallSourceList { items }))
}

pub(super) async fn get_plugin_install_source_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((plugin_id, release_id)): Path<(String, String)>,
    Query(query): Query<PluginInstallSourceQuery>,
) -> Result<Json<PluginInstallSource>, ApiError> {
    let identity =
        require_local_connector_internal_request(&state, &headers, PLUGIN_INSTALL_MANAGE_SCOPE)?;
    let owner_user_id = required_text(Some(query.owner_user_id.as_str()), "owner_user_id")?;
    let internal_audit = PluginManagementInternalAuditGuard::new(
        &identity,
        Some(owner_user_id.as_str()),
        "plugin_install_source",
        plugin_id.as_str(),
        "resolve",
    );
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin install source not found"))?;
    let result = load_install_source(
        &state,
        plugin,
        Some(release_id.as_str()),
        owner_user_id.as_str(),
    )
    .await;
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_RESOLVE_INSTALL_SOURCE,
        owner_user_id.as_str(),
        None,
        plugin_id.as_str(),
        Some(release_id.as_str()),
        if result.is_ok() { "success" } else { "denied" },
        BTreeMap::new(),
    );
    if let Err(error) = state.store.insert_plugin_audit(&audit).await {
        tracing::warn!(
            plugin_id = plugin_id.as_str(),
            release_id = release_id.as_str(),
            owner_user_id = owner_user_id.as_str(),
            error = error.as_str(),
            "persist Plugin install-source audit failed"
        );
    }
    let response = result.map(Json)?;
    internal_audit.succeeded();
    Ok(response)
}

async fn load_install_source(
    state: &AppState,
    plugin: PluginCatalogRecord,
    requested_release_id: Option<&str>,
    owner_user_id: &str,
) -> Result<PluginInstallSource, ApiError> {
    let mut marketplace = state
        .store
        .get_plugin_marketplace(plugin.marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin install source not found"))?;
    if marketplace.visibility == PLUGIN_VISIBILITY_PRIVATE
        && marketplace.owner_user_id.as_deref() != Some(owner_user_id)
    {
        return Err(ApiError::not_found("Plugin install source not found"));
    }
    if !is_network_marketplace(&marketplace) {
        return Err(ApiError::conflict(
            "Plugin Marketplace is not enabled and trusted for network installation",
        ));
    }
    let (mut plugin, release) = if requires_verified_catalog_snapshot(&marketplace) {
        let sync = state
            .store
            .get_plugin_catalog_sync(marketplace.id.as_str())
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| {
                ApiError::conflict("network Plugin Marketplace has no verified Catalog snapshot")
            })?;
        let plugin = sync
            .document
            .plugins
            .iter()
            .find(|item| item.id == plugin.id)
            .cloned()
            .ok_or_else(|| ApiError::not_found("Plugin install source not found"))?;
        let release_id = requested_release_id.unwrap_or(plugin.latest_release_id.as_str());
        if release_id != plugin.latest_release_id {
            return Err(ApiError::conflict(
                "only the current signed stable Plugin Release can be installed",
            ));
        }
        let release = sync
            .document
            .releases
            .iter()
            .find(|item| item.id == release_id && item.plugin_id == plugin.id)
            .cloned()
            .ok_or_else(|| ApiError::not_found("Plugin Release not found"))?;
        marketplace.trusted_signing_keys = sync.document.signing_keys;
        marketplace.last_catalog_revision = Some(sync.revision);
        marketplace.last_synced_at = Some(sync.synced_at);
        (plugin, release)
    } else if is_direct_admin_registry_marketplace(&marketplace) {
        let release_id = requested_release_id.unwrap_or(plugin.latest_release_id.as_str());
        if release_id != plugin.latest_release_id {
            return Err(ApiError::conflict(
                "only the current signed stable Plugin Release can be installed",
            ));
        }
        let release = state
            .store
            .get_plugin_release(release_id)
            .await
            .map_err(ApiError::internal)?
            .filter(|release| release.plugin_id == plugin.id)
            .ok_or_else(|| ApiError::not_found("Plugin Release not found"))?;
        require_approved_publisher_identity(state, &marketplace, &plugin.publisher).await?;
        require_approved_publisher_release_key(
            state,
            &marketplace,
            &plugin.publisher,
            release.signature.key_id.as_str(),
        )
        .await?;
        (plugin, release)
    } else {
        return Err(ApiError::conflict(
            "network Plugin Marketplace requires a verified Catalog snapshot",
        ));
    };
    apply_marketplace_catalog_scope(&marketplace, &mut plugin);
    ensure_catalog_visible_to_owner(owner_user_id, &plugin)?;
    if !plugin.license.redistributable || plugin.license.reviewed_at.is_none() {
        return Err(ApiError::conflict(
            "Plugin license is not approved for artifact proxy installation",
        ));
    }
    let preference = state
        .store
        .get_user_plugin_preference(owner_user_id, plugin.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    if preference.as_ref().is_some_and(|preference| {
        preference.owner_user_id != owner_user_id || preference.plugin_id != plugin.id
    }) {
        return Err(ApiError::conflict(
            "Plugin preference identity differs from the requested owner or Plugin",
        ));
    }
    validate_install_source(&marketplace, &plugin, &release)?;
    Ok(PluginInstallSource {
        marketplace,
        catalog: plugin,
        release,
        preference,
    })
}

fn validate_install_source(
    marketplace: &PluginMarketplaceRecord,
    plugin: &PluginCatalogRecord,
    release: &PluginReleaseRecord,
) -> Result<(), ApiError> {
    if !is_network_marketplace(marketplace) {
        return Err(ApiError::conflict(
            "Plugin Marketplace is not enabled and trusted for network installation",
        ));
    }
    if requires_verified_catalog_snapshot(marketplace) {
        let catalog_url = marketplace.catalog_url.as_deref().ok_or_else(|| {
            ApiError::conflict("network Plugin Marketplace is missing catalog_url")
        })?;
        validate_https_url(catalog_url, "Marketplace catalog_url")?;
        if marketplace
            .last_catalog_revision
            .as_deref()
            .is_none_or(str::is_empty)
            || marketplace
                .last_synced_at
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(ApiError::conflict(
                "network Plugin Marketplace has no synchronized Catalog revision",
            ));
        }
    } else if !is_direct_admin_registry_marketplace(marketplace) {
        return Err(ApiError::conflict(
            "network Plugin Marketplace requires a verified Catalog snapshot",
        ));
    }
    if plugin.marketplace_id != marketplace.id
        || release.plugin_id != plugin.id
        || plugin.latest_release_id != release.id
        || release.release_channel != "stable"
        || release.revoked_at.is_some()
    {
        return Err(ApiError::conflict(
            "Plugin Marketplace, Catalog, and Release identities are inconsistent",
        ));
    }
    if release.version != release.normalized_manifest.version
        || plugin.name != release.normalized_manifest.name
        || release.manifest_schema_version != release.normalized_manifest.schema_version
        || release.dependencies != release.normalized_manifest.dependencies
        || release.permissions != release.normalized_manifest.permissions
        || release.supported_platforms
            != release.normalized_manifest.dependencies.supported_platforms
    {
        return Err(ApiError::conflict(
            "Plugin Release identity differs from its normalized Manifest",
        ));
    }
    if let Some(sbom_ref) = release.sbom_ref.as_deref() {
        if sbom_ref.contains("://") || normalize_plugin_relative_path(sbom_ref).is_err() {
            return Err(ApiError::conflict(
                "network Plugin Release SBOM must be a Plugin-relative artifact path",
            ));
        }
    }
    validate_artifact_url(release.artifact_ref.as_str(), "Plugin artifact_ref")?;
    let key = marketplace
        .trusted_signing_keys
        .iter()
        .find(|key| key.key_id == release.signature.key_id)
        .ok_or_else(|| ApiError::conflict("Plugin Release signing key is not trusted"))?;
    verify_plugin_release_signature(
        PluginReleaseVerificationContext {
            plugin_id: plugin.id.as_str(),
            version: release.version.as_str(),
            marketplace_id: marketplace.id.as_str(),
            publisher_id: plugin.publisher.id.as_str(),
            artifact_sha256: release.artifact_sha256.as_str(),
        },
        &release.normalized_manifest,
        &release.signature,
        key,
    )
    .map_err(|error| ApiError::conflict(format!("Plugin Release signature is invalid: {error}")))
}

fn is_network_marketplace(marketplace: &PluginMarketplaceRecord) -> bool {
    marketplace.enabled
        && marketplace.trust_level == PLUGIN_TRUST_TRUSTED
        && matches!(
            marketplace.source_kind.as_str(),
            PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY | PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
        )
}

fn requires_verified_catalog_snapshot(marketplace: &PluginMarketplaceRecord) -> bool {
    marketplace.catalog_url.is_some()
}

fn is_direct_admin_registry_marketplace(marketplace: &PluginMarketplaceRecord) -> bool {
    marketplace.source_kind == PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
        && marketplace.catalog_url.is_none()
        && marketplace.visibility == PLUGIN_VISIBILITY_PUBLIC
        && marketplace.owner_user_id.is_none()
}

fn validate_https_url(value: &str, field: &str) -> Result<(), ApiError> {
    validate_network_url(value, field, false)
}

fn validate_artifact_url(value: &str, field: &str) -> Result<(), ApiError> {
    validate_network_url(value, field, true)
}

fn validate_network_url(
    value: &str,
    field: &str,
    allow_loopback_http: bool,
) -> Result<(), ApiError> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| ApiError::conflict(format!("{field} is not a valid URL")))?;
    let loopback_host = url.host_str().is_some_and(|host| {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    let loopback_development_url = allow_loopback_http && url.scheme() == "http" && loopback_host;
    if (url.scheme() != "https" && !loopback_development_url)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::conflict(format!(
            "{field} must be an HTTPS URL without credentials or fragments, except for an HTTP loopback development URL"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_urls_allow_http_only_for_loopback_development() {
        assert!(validate_artifact_url(
            "https://registry.npmjs.org/demo/-/demo-1.0.0.tgz",
            "artifact"
        )
        .is_ok());
        assert!(validate_artifact_url(
            "http://127.0.0.1:39260/api/plugin-artifacts/demo",
            "artifact"
        )
        .is_ok());
        assert!(validate_artifact_url(
            "http://localhost:39260/api/plugin-artifacts/demo",
            "artifact"
        )
        .is_ok());
        assert!(validate_artifact_url(
            "http://registry.npmjs.org/demo/-/demo-1.0.0.tgz",
            "artifact"
        )
        .is_err());
        assert!(validate_artifact_url(
            "https://user@registry.npmjs.org/demo/-/demo-1.0.0.tgz",
            "artifact"
        )
        .is_err());
        assert!(
            validate_artifact_url("https://plugins.example.com/demo.zip#fragment", "artifact")
                .is_err()
        );
    }

    #[test]
    fn marketplace_catalog_urls_remain_https_only() {
        assert!(validate_https_url("https://plugins.example.com/catalog.json", "catalog").is_ok());
        assert!(validate_https_url("http://127.0.0.1:39260/catalog.json", "catalog").is_err());
    }

    #[test]
    fn only_enabled_trusted_registry_marketplaces_are_network_install_sources() {
        let mut marketplace = PluginMarketplaceRecord {
            id: "marketplace".to_string(),
            name: "marketplace".to_string(),
            owner_user_id: None,
            visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
            source_kind: PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY.to_string(),
            catalog_url: Some("https://plugins.example.com/catalog.json".to_string()),
            enabled: true,
            trust_level: PLUGIN_TRUST_TRUSTED.to_string(),
            trusted_signing_keys: Vec::new(),
            last_catalog_revision: Some("revision".to_string()),
            last_synced_at: Some("2026-07-25T00:00:00Z".to_string()),
        };
        assert!(is_network_marketplace(&marketplace));
        marketplace.trust_level = PLUGIN_TRUST_UNTRUSTED.to_string();
        assert!(!is_network_marketplace(&marketplace));
        marketplace.trust_level = PLUGIN_TRUST_TRUSTED.to_string();
        marketplace.enabled = false;
        assert!(!is_network_marketplace(&marketplace));
    }

    #[test]
    fn public_admin_registry_without_catalog_url_is_directly_curated() {
        let marketplace = PluginMarketplaceRecord {
            id: "marketplace".to_string(),
            name: "marketplace".to_string(),
            owner_user_id: None,
            visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
            source_kind: PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY.to_string(),
            catalog_url: None,
            enabled: true,
            trust_level: PLUGIN_TRUST_TRUSTED.to_string(),
            trusted_signing_keys: Vec::new(),
            last_catalog_revision: None,
            last_synced_at: None,
        };
        assert!(is_direct_admin_registry_marketplace(&marketplace));
        assert!(!requires_verified_catalog_snapshot(&marketplace));
    }
}
