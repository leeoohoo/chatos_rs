async fn validate_catalog_against_store(
    state: &AppState,
    document: &PluginCatalogDocument,
    allow_committed_snapshot_repair: bool,
) -> Result<(), ApiError> {
    let plugin_ids = document
        .plugins
        .iter()
        .map(|plugin| plugin.id.clone())
        .collect::<Vec<_>>();
    let release_ids = document
        .releases
        .iter()
        .map(|release| release.id.clone())
        .collect::<Vec<_>>();
    let release_plugin_ids = document
        .releases
        .iter()
        .map(|release| release.plugin_id.clone())
        .collect::<Vec<_>>();
    let release_versions = document
        .releases
        .iter()
        .map(|release| release.version.clone())
        .collect::<Vec<_>>();
    let (plugins, releases_by_id, releases_by_version, snapshots) = tokio::try_join!(
        state.store.list_plugin_catalog_entries_by_ids(&plugin_ids),
        state.store.list_plugin_releases_any_state_by_ids(&release_ids),
        state
            .store
            .list_plugin_releases_by_versions(&release_plugin_ids, &release_versions),
        state
            .store
            .list_plugin_component_snapshots_by_release_ids(&release_ids),
    )
    .map_err(ApiError::internal)?;
    let plugins = plugins
        .into_iter()
        .map(|plugin| (plugin.id.clone(), plugin))
        .collect::<HashMap<_, _>>();
    let releases_by_id = releases_by_id
        .into_iter()
        .map(|release| (release.id.clone(), release))
        .collect::<HashMap<_, _>>();
    let releases_by_version = releases_by_version
        .into_iter()
        .map(|release| ((release.plugin_id.clone(), release.version.clone()), release))
        .collect::<HashMap<_, _>>();
    let mut snapshots_by_release: HashMap<String, Vec<PluginComponentSnapshot>> = HashMap::new();
    for snapshot in snapshots {
        snapshots_by_release
            .entry(snapshot.release_id.clone())
            .or_default()
            .push(snapshot);
    }
    let mut incoming_snapshots_by_release: HashMap<String, Vec<PluginComponentSnapshot>> =
        HashMap::new();
    for snapshot in &document.component_snapshots {
        incoming_snapshots_by_release
            .entry(snapshot.release_id.clone())
            .or_default()
            .push(snapshot.clone());
    }

    for plugin in &document.plugins {
        if let Some(existing) = plugins.get(&plugin.id) {
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
        if let Some(existing) = releases_by_id.get(&release.id) {
            validate_release_progression(existing, release)?;
        }
        if let Some(existing) =
            releases_by_version.get(&(release.plugin_id.clone(), release.version.clone()))
        {
            if existing.id != release.id {
                return Err(ApiError::conflict(format!(
                    "Catalog Release version conflicts with immutable stored Release {}@{}",
                    release.plugin_id, release.version
                )));
            }
            validate_release_progression(existing, release)?;
        }
        let mut incoming_snapshots = incoming_snapshots_by_release
            .remove(&release.id)
            .unwrap_or_default();
        incoming_snapshots.sort_by(|left, right| {
            left.component
                .component_key
                .cmp(&right.component.component_key)
        });
        let mut existing_snapshots = snapshots_by_release
            .remove(&release.id)
            .unwrap_or_default();
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

pub(crate) fn is_syncable_network_marketplace(marketplace: &PluginMarketplaceRecord) -> bool {
    marketplace.enabled
        && marketplace.trust_level == PLUGIN_TRUST_TRUSTED
        && marketplace.catalog_url.is_some()
        && matches!(
            marketplace.source_kind.as_str(),
            PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY | PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
        )
}
