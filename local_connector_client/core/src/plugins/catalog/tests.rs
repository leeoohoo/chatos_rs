// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
    assert_eq!(presentations.latest_version, "1.32.0");
    assert_eq!(
        presentations.latest_release_id,
        "bundled-release-presentations-1-32-0"
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
