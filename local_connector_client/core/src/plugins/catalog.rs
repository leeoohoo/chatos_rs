// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, bail, Context, Result};
use chatos_plugin_management_sdk::{
    PluginComponentKind, PluginInstallSourceList, PluginInstallStatus, UserPluginPreferenceRecord,
};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::skills::internal_skill_catalog;

use super::{
    verify_plugin_install_source, LocalInstalledPlugin, LocalPluginStatusSnapshot,
    PluginAutoUpdateRecord, PluginAutoUpdateState, PluginRuntimeTelemetrySnapshot,
    PluginTransactionOperation, PluginTransactionRecord,
};

const BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";
const BUNDLED_MARKETPLACE_NAME: &str = "ChatOS Bundled";
pub(super) const BUNDLED_SIGNATURE_KEY_ID: &str = "chatos-bundled-attestation-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BundledPluginSpec {
    pub plugin_id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub skill_ids: Vec<String>,
    pub release_id: String,
    pub release_version: String,
    pub release_epoch: String,
    pub artifact_revision: String,
    pub catalog_revision: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledPluginCatalogDocument {
    schema_version: u32,
    catalog_revision: String,
    release_version: String,
    release_epoch: String,
    artifact_revision: String,
    plugins: Vec<BundledPluginCatalogItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledPluginCatalogItem {
    name: String,
    display_name: String,
    description: String,
    category: String,
    skill_ids: Vec<String>,
    #[serde(default)]
    release_version: Option<String>,
    #[serde(default)]
    release_epoch: Option<String>,
    #[serde(default)]
    artifact_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalPluginStoreSnapshot {
    pub schema_version: u32,
    pub catalog_revision: String,
    pub marketplace_id: String,
    pub marketplace_name: String,
    pub bundled_install_available: bool,
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
    let bundled = bundled_plugin_catalog()?;
    let LocalPluginStatusSnapshot {
        registry,
        transactions,
        runtime,
    } = status;
    let mut installed = registry.plugins;
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

    let mut items = Vec::with_capacity(bundled.plugins.len() + installed.len());
    for source in bundled.plugins {
        let plugin_id = format!("bundled-plugin-{}", source.name);
        let installation = installed.remove(plugin_id.as_str());
        let active_transaction = active_by_plugin.get(plugin_id.as_str()).cloned();
        let latest_transaction = latest_by_plugin.get(plugin_id.as_str()).cloned();
        let latest_version = source
            .release_version
            .unwrap_or_else(|| bundled.release_version.clone());
        let update_available = installation
            .as_ref()
            .and_then(|plugin| plugin.active_version.as_deref())
            .is_some_and(|current| version_is_older(current, latest_version.as_str()));
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
        items.push(LocalPluginStoreItem {
            plugin_id: plugin_id.clone(),
            marketplace_id: BUNDLED_MARKETPLACE_ID.to_string(),
            name: source.name.clone(),
            display_name: source.display_name,
            description: source.description,
            category: source.category,
            publisher: "ChatOS".to_string(),
            visibility: "public".to_string(),
            featured: is_featured_bundled_plugin(source.name.as_str()),
            latest_release_id: bundled_release_id(source.name.as_str(), latest_version.as_str()),
            latest_version,
            published_at: source
                .release_epoch
                .unwrap_or_else(|| bundled.release_epoch.clone()),
            artifact_revision: source
                .artifact_revision
                .unwrap_or_else(|| bundled.artifact_revision.clone()),
            skill_ids: source.skill_ids,
            install_source: "bundled".to_string(),
            install_available: false,
            lifecycle_status,
            update_available,
            rollback_available,
            preference: None,
            auto_update_state: None,
            installation,
            active_transaction,
            latest_transaction,
        });
    }

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
        schema_version: bundled.schema_version,
        catalog_revision: bundled.catalog_revision,
        marketplace_id: BUNDLED_MARKETPLACE_ID.to_string(),
        marketplace_name: BUNDLED_MARKETPLACE_NAME.to_string(),
        bundled_install_available: false,
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
        if existing_index.is_some_and(|index| snapshot.items[index].install_source == "bundled") {
            bail!("network Plugin ID collides with the bundled Catalog");
        }
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
    snapshot.network_install_available = !seen.is_empty();
    snapshot.network_catalog_error = None;
    if !revisions.is_empty() {
        snapshot.marketplace_name = "ChatOS Bundled + Trusted Marketplaces".to_string();
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

pub(super) fn bundled_plugin_spec(plugin_id: &str) -> Result<BundledPluginSpec> {
    let catalog = bundled_plugin_catalog()?;
    let source = catalog
        .plugins
        .iter()
        .find(|plugin| format!("bundled-plugin-{}", plugin.name) == plugin_id)
        .with_context(|| {
            format!("bundled Plugin is not present in the embedded Catalog: {plugin_id}")
        })?;
    let release_version = source
        .release_version
        .clone()
        .unwrap_or_else(|| catalog.release_version.clone());
    Ok(BundledPluginSpec {
        plugin_id: plugin_id.to_string(),
        name: source.name.clone(),
        display_name: source.display_name.clone(),
        description: source.description.clone(),
        category: source.category.clone(),
        skill_ids: source.skill_ids.clone(),
        release_id: bundled_release_id(source.name.as_str(), release_version.as_str()),
        release_version,
        release_epoch: source
            .release_epoch
            .clone()
            .unwrap_or_else(|| catalog.release_epoch.clone()),
        artifact_revision: source
            .artifact_revision
            .clone()
            .unwrap_or_else(|| catalog.artifact_revision.clone()),
        catalog_revision: catalog.catalog_revision,
    })
}

fn bundled_plugin_catalog() -> Result<BundledPluginCatalogDocument> {
    let catalog = serde_json::from_str::<BundledPluginCatalogDocument>(include_str!(
        "../../../plugin_bundles/catalog/bundled-plugin-catalog.json"
    ))
    .context("decode embedded bundled Plugin catalog")?;
    if catalog.schema_version != 1 {
        bail!(
            "unsupported bundled Plugin catalog schema version: {}",
            catalog.schema_version
        );
    }
    let skill_catalog = internal_skill_catalog()?;
    if catalog.catalog_revision.trim().is_empty()
        || catalog.catalog_revision != skill_catalog.catalog_revision
        || catalog.plugins.len() != 12
    {
        bail!("embedded bundled Plugin catalog is incomplete or out of sync");
    }
    validate_stable_version(catalog.release_version.as_str(), "default Release")?;
    let expected_skills = skill_catalog
        .skills
        .iter()
        .map(|skill| skill.skill_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    let mut mapped_skills = BTreeSet::new();
    for plugin in &catalog.plugins {
        if !is_safe_slug(plugin.name.as_str())
            || plugin.display_name.trim().is_empty()
            || plugin.description.trim().is_empty()
            || plugin.category.trim().is_empty()
            || plugin.skill_ids.is_empty()
            || !names.insert(plugin.name.as_str())
        {
            bail!("embedded bundled Plugin catalog contains an invalid entry");
        }
        validate_stable_version(
            plugin
                .release_version
                .as_deref()
                .unwrap_or(catalog.release_version.as_str()),
            plugin.name.as_str(),
        )?;
        for skill_id in &plugin.skill_ids {
            if !expected_skills.contains(skill_id.as_str())
                || !mapped_skills.insert(skill_id.as_str())
            {
                bail!("bundled Plugin catalog has an invalid Skill mapping");
            }
        }
    }
    if mapped_skills != expected_skills {
        bail!("bundled Plugin catalog does not cover every internal Skill exactly once");
    }
    Ok(catalog)
}

fn validate_stable_version(value: &str, label: &str) -> Result<()> {
    let version = Version::parse(value)
        .with_context(|| format!("bundled Plugin {label} has an invalid version"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(anyhow!(
            "bundled Plugin {label} version must be stable x.y.z"
        ));
    }
    Ok(())
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

fn is_safe_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn bundled_release_id(name: &str, version: &str) -> String {
    format!("bundled-release-{name}-{}", version.replace('.', "-"))
}

fn is_featured_bundled_plugin(name: &str) -> bool {
    matches!(name, "documents" | "pdf" | "browser" | "computer-use")
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
mod tests {
    use super::*;
    use crate::plugins::tests::fixtures::{ArchiveMutation, TestSigner};
    use crate::plugins::{LocalPluginRegistry, PluginTransactionJournal};
    use tempfile::TempDir;

    #[test]
    fn bundled_store_catalog_covers_featured_categories_and_all_internal_skills() {
        let snapshot = local_plugin_store_snapshot(LocalPluginStatusSnapshot {
            registry: LocalPluginRegistry::default(),
            transactions: PluginTransactionJournal::default(),
            runtime: Default::default(),
        })
        .expect("Plugin store snapshot");
        assert_eq!(snapshot.items.len(), 12);
        assert_eq!(
            snapshot
                .items
                .iter()
                .flat_map(|plugin| plugin.skill_ids.iter())
                .count(),
            28
        );
        assert_eq!(
            snapshot
                .items
                .iter()
                .filter(|plugin| plugin.featured)
                .count(),
            4
        );
        assert!(snapshot.items.iter().all(|plugin| {
            plugin.visibility == "public" && plugin.lifecycle_status == "not_installed"
        }));
        let presentations = snapshot
            .items
            .iter()
            .find(|plugin| plugin.name == "presentations")
            .expect("Presentations Plugin");
        assert_eq!(presentations.latest_version, "1.25.0");
        assert_eq!(
            presentations.latest_release_id,
            "bundled-release-presentations-1-25-0"
        );
    }

    #[test]
    fn trusted_network_source_merges_with_the_local_registry_catalog() {
        let temp = TempDir::new().expect("temp directory");
        let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
        let mut source = package.install_source();
        source.preference = Some(UserPluginPreferenceRecord {
            owner_user_id: "owner".to_string(),
            plugin_id: source.catalog.id.clone(),
            enabled: true,
            auto_update: true,
            release_channel: "stable".to_string(),
            enabled_components: Vec::new(),
            updated_at: "2026-07-26T00:00:00Z".to_string(),
        });
        let mut snapshot = local_plugin_store_snapshot(LocalPluginStatusSnapshot {
            registry: LocalPluginRegistry::default(),
            transactions: PluginTransactionJournal::default(),
            runtime: Default::default(),
        })
        .expect("Plugin store snapshot");

        merge_network_plugin_sources(
            &mut snapshot,
            PluginInstallSourceList {
                items: vec![source],
            },
        )
        .expect("merge trusted Marketplace source");

        let item = snapshot
            .items
            .iter()
            .find(|item| item.plugin_id == "plugin-demo")
            .expect("network Plugin item");
        assert_eq!(snapshot.items.len(), 13);
        assert!(snapshot.network_install_available);
        assert_eq!(item.install_source, "network");
        assert!(item.install_available);
        assert_eq!(item.marketplace_id, "trusted-marketplace");
        assert_eq!(item.latest_release_id, "release-1.0.0");
        assert!(item.preference.as_ref().is_some_and(|preference| {
            preference.enabled && preference.auto_update && preference.release_channel == "stable"
        }));
    }

    #[test]
    fn network_source_signature_tampering_is_rejected() {
        let temp = TempDir::new().expect("temp directory");
        let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
        let mut source = package.install_source();
        source.release.artifact_sha256 = "0".repeat(64);
        let mut snapshot = local_plugin_store_snapshot(LocalPluginStatusSnapshot {
            registry: LocalPluginRegistry::default(),
            transactions: PluginTransactionJournal::default(),
            runtime: Default::default(),
        })
        .expect("Plugin store snapshot");

        assert!(merge_network_plugin_sources(
            &mut snapshot,
            PluginInstallSourceList {
                items: vec![source],
            },
        )
        .is_err());
        assert_eq!(snapshot.items.len(), 12);
    }
}
