// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
    let (_, inventory, _) = expected_manifest_and_inventory(&spec).expect("Computer Use inventory");
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
