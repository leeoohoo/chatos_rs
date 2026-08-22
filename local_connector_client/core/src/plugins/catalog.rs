// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    PluginComponentKind, PluginInstallSourceList, PluginInstallStatus, UserPluginPreferenceRecord,
};
use semver::Version;
use serde::Serialize;

use super::{
    verify_plugin_install_source, LocalInstalledPlugin, LocalPluginStatusSnapshot,
    PluginAutoUpdateRecord, PluginAutoUpdateState, PluginRuntimeTelemetrySnapshot,
    PluginTransactionOperation, PluginTransactionRecord,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalPluginStoreSnapshot {
    pub schema_version: u32,
    pub catalog_revision: String,
    pub marketplace_id: String,
    pub marketplace_name: String,
    pub network_install_available: bool,
    #[serde(default)]
    pub network_catalog_error: Option<String>,
    #[serde(default)]
    pub auto_update_error: Option<String>,
    #[serde(default)]
    pub runtime: PluginRuntimeTelemetrySnapshot,
    pub items: Vec<LocalPluginStoreItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalPluginStoreItem {
    pub plugin_id: String,
    pub marketplace_id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub publisher: String,
    pub visibility: String,
    pub featured: bool,
    pub latest_version: String,
    pub latest_release_id: String,
    pub published_at: String,
    pub artifact_revision: String,
    pub skill_ids: Vec<String>,
    pub install_source: String,
    pub install_available: bool,
    pub execution_type: String,
    pub requires_local_install: bool,
    pub lifecycle_status: String,
    pub update_available: bool,
    pub rollback_available: bool,
    #[serde(default)]
    pub preference: Option<UserPluginPreferenceRecord>,
    #[serde(default)]
    pub auto_update_state: Option<PluginAutoUpdateRecord>,
    #[serde(default)]
    pub installation: Option<LocalInstalledPlugin>,
    #[serde(default)]
    pub active_transaction: Option<PluginTransactionRecord>,
    #[serde(default)]
    pub latest_transaction: Option<PluginTransactionRecord>,
}

pub fn local_plugin_store_snapshot(
    status: LocalPluginStatusSnapshot,
) -> Result<LocalPluginStoreSnapshot> {
    let LocalPluginStatusSnapshot {
        registry,
        transactions,
        runtime,
    } = status;
    let installed = registry.plugins;
    let active_by_plugin = transactions
        .active
        .values()
        .map(|transaction| (transaction.plugin_id.clone(), transaction.clone()))
        .collect::<BTreeMap<_, _>>();
    let latest_by_plugin = transactions.history.iter().fold(
        BTreeMap::<String, PluginTransactionRecord>::new(),
        |mut latest, transaction| {
            latest.insert(transaction.plugin_id.clone(), transaction.clone());
            latest
        },
    );

    let mut items = Vec::with_capacity(installed.len());
    for (_, installation) in installed {
        let plugin_id = installation.plugin_id.clone();
        let active_transaction = active_by_plugin.get(plugin_id.as_str()).cloned();
        let latest_transaction = latest_by_plugin.get(plugin_id.as_str()).cloned();
        let active_version = installation.active_version.clone().unwrap_or_default();
        let active_release = installation
            .versions
            .get(active_version.as_str())
            .map(|version| version.release_id.clone())
            .unwrap_or_default();
        let skill_ids = installation
            .versions
            .get(active_version.as_str())
            .into_iter()
            .flat_map(|version| version.inventory.components.iter())
            .filter_map(|component| {
                component
                    .metadata
                    .get("skill_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        let rollback_available = has_verified_rollback_target(&installation);
        let lifecycle_status = lifecycle_status(
            Some(&installation),
            active_transaction.as_ref(),
            latest_transaction.as_ref(),
            false,
            rollback_available,
        );
        items.push(LocalPluginStoreItem {
            plugin_id,
            marketplace_id: installation.marketplace_id.clone(),
            name: installation.plugin_name.clone(),
            display_name: display_name_from_slug(installation.plugin_name.as_str()),
            description: format!(
                "已从 {} Marketplace 安装到这台设备。",
                installation.marketplace_id
            ),
            category: "Developer Tools".to_string(),
            publisher: installation.marketplace_id.clone(),
            visibility: "personal".to_string(),
            featured: false,
            latest_version: active_version,
            latest_release_id: active_release,
            published_at: String::new(),
            artifact_revision: String::new(),
            skill_ids,
            install_source: "installed".to_string(),
            install_available: false,
            execution_type: "local_npm_mcp".to_string(),
            requires_local_install: true,
            lifecycle_status,
            update_available: false,
            rollback_available,
            preference: None,
            auto_update_state: None,
            installation: Some(installation),
            active_transaction,
            latest_transaction,
        });
    }

    items.sort_by(|left, right| {
        right
            .featured
            .cmp(&left.featured)
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    Ok(LocalPluginStoreSnapshot {
        schema_version: 2,
        catalog_revision: "npm-mcp-only".to_string(),
        marketplace_id: "trusted-npm-marketplaces".to_string(),
        marketplace_name: "Trusted npm MCP Marketplaces".to_string(),
        network_install_available: false,
        network_catalog_error: None,
        auto_update_error: None,
        runtime,
        items,
    })
}

pub fn merge_network_plugin_sources(
    snapshot: &mut LocalPluginStoreSnapshot,
    sources: PluginInstallSourceList,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut revisions = BTreeSet::new();
    for source in sources.items {
        verify_plugin_install_source(&source).context("verify network Plugin install source")?;
        if source.catalog.latest_release_id != source.release.id
            || source.release.release_channel != "stable"
        {
            bail!("network Plugin install source does not identify the current stable Release");
        }
        if !seen.insert(source.catalog.id.clone()) {
            bail!("network Plugin Catalog contains a duplicate Plugin ID");
        }
        if let Some(revision) = source.marketplace.last_catalog_revision.as_deref() {
            revisions.insert(format!("{}:{revision}", source.marketplace.id));
        }

        let existing_index = snapshot
            .items
            .iter()
            .position(|item| item.plugin_id == source.catalog.id);
        let existing = existing_index.map(|index| snapshot.items.remove(index));
        if existing
            .as_ref()
            .and_then(|item| item.installation.as_ref())
            .is_some_and(|installation| installation.marketplace_id != source.marketplace.id)
        {
            bail!("installed Plugin Marketplace identity differs from the network Catalog");
        }
        let installation = existing.as_ref().and_then(|item| item.installation.clone());
        let active_transaction = existing
            .as_ref()
            .and_then(|item| item.active_transaction.clone());
        let latest_transaction = existing
            .as_ref()
            .and_then(|item| item.latest_transaction.clone());
        let update_available = installation
            .as_ref()
            .and_then(|plugin| plugin.active_version.as_deref())
            .is_some_and(|current| version_is_older(current, source.release.version.as_str()));
        let rollback_available = installation
            .as_ref()
            .is_some_and(has_verified_rollback_target);
        let lifecycle_status = lifecycle_status(
            installation.as_ref(),
            active_transaction.as_ref(),
            latest_transaction.as_ref(),
            update_available,
            rollback_available,
        );
        let skill_ids = source
            .release
            .components
            .iter()
            .filter(|component| component.kind == PluginComponentKind::SkillCollection)
            .map(|component| {
                component
                    .metadata
                    .get("skill_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(component.component_key.as_str())
                    .to_string()
            })
            .collect::<Vec<_>>();
        snapshot.items.push(LocalPluginStoreItem {
            plugin_id: source.catalog.id,
            marketplace_id: source.marketplace.id,
            name: source.catalog.name,
            display_name: source.catalog.display_name,
            description: source.catalog.description,
            category: source.catalog.interface.category,
            publisher: source.catalog.publisher.name,
            visibility: source.catalog.visibility,
            featured: source.catalog.featured,
            latest_version: source.release.version,
            latest_release_id: source.release.id,
            published_at: source.release.published_at,
            artifact_revision: source.release.artifact_sha256,
            skill_ids,
            install_source: "network".to_string(),
            install_available: true,
            execution_type: "local_npm_mcp".to_string(),
            requires_local_install: true,
            lifecycle_status,
            update_available,
            rollback_available,
            preference: source.preference,
            auto_update_state: None,
            installation,
            active_transaction,
            latest_transaction,
        });
    }
    snapshot.network_install_available = snapshot
        .items
        .iter()
        .any(|item| item.install_source == "network" && item.install_available);
    snapshot.network_catalog_error = None;
    if !revisions.is_empty() {
        snapshot.marketplace_name = "Trusted npm MCP Marketplaces".to_string();
        snapshot.catalog_revision = format!(
            "{}+{}",
            snapshot.catalog_revision,
            revisions.into_iter().collect::<Vec<_>>().join(",")
        );
    }
    snapshot.items.sort_by(|left, right| {
        right
            .featured
            .cmp(&left.featured)
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    Ok(())
}

pub fn merge_auto_update_state(
    snapshot: &mut LocalPluginStoreSnapshot,
    state: &PluginAutoUpdateState,
) {
    for item in &mut snapshot.items {
        item.auto_update_state = state.plugins.get(item.plugin_id.as_str()).cloned();
    }
}

fn lifecycle_status(
    installation: Option<&LocalInstalledPlugin>,
    active_transaction: Option<&PluginTransactionRecord>,
    latest_transaction: Option<&PluginTransactionRecord>,
    update_available: bool,
    rollback_available: bool,
) -> String {
    if let Some(transaction) = active_transaction {
        return install_status_name(transaction.status).to_string();
    }
    let active = installation.and_then(|plugin| plugin.active_version.as_ref());
    if active.is_none() {
        return if latest_transaction
            .is_some_and(|transaction| transaction.status == PluginInstallStatus::Rejected)
        {
            "install_failed"
        } else {
            "not_installed"
        }
        .to_string();
    }
    if latest_transaction.is_some_and(|transaction| {
        transaction.status == PluginInstallStatus::Rejected
            && matches!(
                transaction.operation,
                PluginTransactionOperation::Update | PluginTransactionOperation::Rollback
            )
    }) {
        return if rollback_available {
            "update_failed_rollback_available"
        } else {
            "update_failed"
        }
        .to_string();
    }
    if update_available {
        "needs_update".to_string()
    } else {
        "installed".to_string()
    }
}

fn install_status_name(status: PluginInstallStatus) -> &'static str {
    match status {
        PluginInstallStatus::NotInstalled => "not_installed",
        PluginInstallStatus::Downloading => "downloading",
        PluginInstallStatus::Verifying => "verifying",
        PluginInstallStatus::Rejected => "rejected",
        PluginInstallStatus::Installing => "installing",
        PluginInstallStatus::Installed => "installed",
        PluginInstallStatus::Updating => "updating",
        PluginInstallStatus::RollingBack => "rolling_back",
        PluginInstallStatus::Uninstalling => "uninstalling",
    }
}

fn has_verified_rollback_target(plugin: &LocalInstalledPlugin) -> bool {
    plugin
        .previous_version
        .as_ref()
        .is_some_and(|version| plugin.versions.contains_key(version))
}

fn version_is_older(current: &str, latest: &str) -> bool {
    Version::parse(current)
        .ok()
        .zip(Version::parse(latest).ok())
        .is_some_and(|(current, latest)| current < latest)
}

fn display_name_from_slug(value: &str) -> String {
    value
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| format!("{}{}", first.to_ascii_uppercase(), chars.as_str()))
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
