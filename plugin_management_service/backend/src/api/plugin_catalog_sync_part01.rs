async fn validate_catalog_against_store(
    state: &AppState,
    document: &PluginCatalogDocument,
    allow_committed_snapshot_repair: bool,
) -> Result<(), ApiError> {
    for plugin in &document.plugins {
        if let Some(existing) = state
            .store
            .get_plugin_catalog_entry(plugin.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if existing.marketplace_id != document.marketplace_id
                || existing.name != plugin.name
                || existing.plugin_key != plugin.plugin_key
                || existing.publisher.id != plugin.publisher.id
                || existing.created_at != plugin.created_at
            {
                return Err(ApiError::conflict(format!(
                    "Catalog Plugin identity conflicts with stored record {}",
                    plugin.id
                )));
            }
        }
    }
    for release in &document.releases {
        if let Some(existing) = state
            .store
            .get_plugin_release(release.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            validate_release_progression(&existing, release)?;
        }
        if let Some(existing) = state
            .store
            .find_plugin_release_by_version(release.plugin_id.as_str(), release.version.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if existing.id != release.id {
                return Err(ApiError::conflict(format!(
                    "Catalog Release version conflicts with immutable stored Release {}@{}",
                    release.plugin_id, release.version
                )));
            }
            validate_release_progression(&existing, release)?;
        }
        let mut incoming_snapshots = document
            .component_snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.plugin_id == release.plugin_id && snapshot.release_id == release.id
            })
            .cloned()
            .collect::<Vec<_>>();
        incoming_snapshots.sort_by(|left, right| {
            left.component
                .component_key
                .cmp(&right.component.component_key)
        });
        let mut existing_snapshots = state
            .store
            .list_plugin_component_snapshots(release.plugin_id.as_str(), release.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        existing_snapshots.sort_by(|left, right| {
            left.component
                .component_key
                .cmp(&right.component.component_key)
        });
        if !allow_committed_snapshot_repair
            && !existing_snapshots.is_empty()
            && existing_snapshots != incoming_snapshots
        {
            return Err(ApiError::conflict(format!(
                "Catalog component snapshots conflict with immutable stored Release {}",
                release.id
            )));
        }
    }
    Ok(())
}

async fn materialize_catalog(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    document: &PluginCatalogDocument,
) -> Result<(), ApiError> {
    let mut staged_release_ids = Vec::new();
    for release in &document.releases {
        let ready = state
            .store
            .get_plugin_release(release.id.as_str())
            .await
            .map_err(ApiError::internal)?
            .is_some();
        if !ready {
            staged_release_ids.push(release.id.clone());
        }
        match state
            .store
            .get_plugin_release_any_state(release.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            Some(existing) => {
                if !ready {
                    state
                        .store
                        .set_plugin_release_publication_ready(release.id.as_str(), false)
                        .await
                        .map_err(ApiError::internal)?;
                }
                if existing != *release {
                    state
                        .store
                        .replace_plugin_release(release)
                        .await
                        .map_err(ApiError::internal)?;
                }
            }
            None => state
                .store
                .insert_plugin_release_pending(release)
                .await
                .map_err(ApiError::internal)?,
        }
        let snapshots = document
            .component_snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.plugin_id == release.plugin_id && snapshot.release_id == release.id
            })
            .cloned()
            .collect::<Vec<_>>();
        state
            .store
            .replace_plugin_component_snapshots(
                release.plugin_id.as_str(),
                release.id.as_str(),
                snapshots.as_slice(),
            )
            .await
            .map_err(ApiError::internal)?;
    }
    for release_id in staged_release_ids {
        state
            .store
            .set_plugin_release_publication_ready(release_id.as_str(), true)
            .await
            .map_err(ApiError::internal)?;
    }
    for plugin in &document.plugins {
        let mut plugin = plugin.clone();
        apply_marketplace_catalog_scope(marketplace, &mut plugin);
        state
            .store
            .replace_plugin_catalog_entry(&plugin)
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(())
}

pub(crate) fn is_syncable_network_marketplace(marketplace: &PluginMarketplaceRecord) -> bool {
    marketplace.enabled
        && marketplace.trust_level == PLUGIN_TRUST_TRUSTED
        && marketplace.catalog_url.is_some()
        && matches!(
            marketplace.source_kind.as_str(),
            PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY | PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
        )
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}
