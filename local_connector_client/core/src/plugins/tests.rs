// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::Duration;

use chatos_plugin_management_sdk::*;
use tempfile::TempDir;

use super::journal::begin_transaction;
use super::verifier::verify_plugin_package;
use super::*;

pub(super) mod fixtures;

use fixtures::*;

#[test]
fn installs_updates_rolls_back_and_uninstalls_atomically() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package_v1 = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
    let package_v2 = signer.package(temp.path(), "1.1.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugin-store"));

    let first = installer
        .install_package(package_v1.install_request())
        .expect("install v1");
    assert_eq!(first.installed_version.version, "1.0.0");
    assert!(first
        .installation_path
        .join("skills/demo/SKILL.md")
        .is_file());

    let restarted = PluginInstaller::new(installer.plugin_root().to_path_buf());
    assert_eq!(
        restarted
            .active_installation(PLUGIN_ID)
            .expect("load active installation")
            .expect("active Plugin")
            .version
            .version,
        "1.0.0"
    );

    restarted
        .install_package(package_v2.install_request())
        .expect("update to v2");
    assert!(restarted
        .install_package(package_v1.install_request())
        .expect_err("downgrade must fail")
        .to_string()
        .contains("downgrade"));
    assert_eq!(
        restarted
            .rollback(PLUGIN_ID)
            .expect("rollback to v1")
            .version
            .version,
        "1.0.0"
    );
    assert_eq!(
        restarted
            .rollback(PLUGIN_ID)
            .expect("return to v2")
            .version
            .version,
        "1.1.0"
    );

    fs::write(
        first.installation_path.join("skills/demo/SKILL.md"),
        "tampered",
    )
    .expect("tamper old version");
    assert!(restarted.rollback(PLUGIN_ID).is_err());
    assert_eq!(
        restarted
            .active_installation(PLUGIN_ID)
            .expect("active after rejected rollback")
            .expect("active Plugin")
            .version
            .version,
        "1.1.0"
    );

    assert!(restarted.uninstall(PLUGIN_ID).expect("uninstall Plugin"));
    assert!(restarted
        .active_installation(PLUGIN_ID)
        .expect("read after uninstall")
        .is_none());
    assert!(!first
        .installation_path
        .parent()
        .expect("version parent")
        .exists());
    let status = restarted.status_snapshot().expect("Plugin status snapshot");
    assert!(status.transactions.active.is_empty());
    assert_eq!(status.transactions.history.len(), 6);
    assert!(status
        .transactions
        .history
        .iter()
        .any(|record| record.status == PluginInstallStatus::Rejected));
    assert_eq!(
        status
            .transactions
            .history
            .last()
            .map(|record| record.status),
        Some(PluginInstallStatus::NotInstalled)
    );
}

#[test]
fn network_download_transaction_advances_through_verification_and_installation() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
    let source = package.install_source();
    let installer = PluginInstaller::new(temp.path().join("plugin-store"));
    let pending = installer
        .begin_network_install(&source.marketplace, &source.catalog, &source.release)
        .expect("begin network download transaction");
    let status = installer.status_snapshot().expect("download status");
    let active = status
        .transactions
        .active
        .get(pending.transaction_id.as_str())
        .expect("active download transaction");
    assert_eq!(active.status, PluginInstallStatus::Downloading);
    assert_eq!(active.downloaded_bytes, 0);
    assert_eq!(active.total_bytes, None);
    assert_eq!(
        active.relative_storage_path.as_deref(),
        Some(pending.relative_download_path.as_str())
    );

    let package_path = installer
        .plugin_root()
        .join(pending.relative_download_path.as_str());
    fs::create_dir_all(package_path.parent().expect("download parent"))
        .expect("create download parent");
    fs::copy(package.package_path(), package_path.as_path()).expect("stage downloaded artifact");
    let archive_bytes = fs::metadata(package_path.as_path())
        .expect("downloaded artifact metadata")
        .len();
    installer
        .update_network_download_progress(&pending, archive_bytes / 2, Some(archive_bytes))
        .expect("persist partial download progress");
    let status = installer
        .status_snapshot()
        .expect("partial download status");
    let active = status
        .transactions
        .active
        .get(pending.transaction_id.as_str())
        .expect("active partial download transaction");
    assert_eq!(active.downloaded_bytes, archive_bytes / 2);
    assert_eq!(active.total_bytes, Some(archive_bytes));
    assert!(installer
        .update_network_download_progress(&pending, archive_bytes / 4, Some(archive_bytes))
        .is_err());
    assert!(installer
        .update_network_download_progress(&pending, archive_bytes / 2, Some(archive_bytes + 1))
        .is_err());
    installer
        .update_network_download_progress(&pending, archive_bytes, Some(archive_bytes))
        .expect("persist completed download progress");
    let outcome = installer
        .install_downloaded_package(
            pending.clone(),
            PluginInstallRequest {
                marketplace: &source.marketplace,
                catalog: &source.catalog,
                release: &source.release,
                package_path: package_path.as_path(),
            },
        )
        .expect("install downloaded artifact");
    assert_eq!(outcome.installed_version.version, "1.0.0");
    let status = installer.status_snapshot().expect("installed status");
    assert!(status.transactions.active.is_empty());
    assert_eq!(
        status
            .transactions
            .history
            .last()
            .map(|record| record.status),
        Some(PluginInstallStatus::Installed)
    );
    let completed = status
        .transactions
        .history
        .last()
        .expect("completed download transaction");
    assert_eq!(completed.downloaded_bytes, archive_bytes);
    assert_eq!(completed.total_bytes, Some(archive_bytes));
}

#[test]
fn network_install_rejects_artifact_without_completed_download_progress() {
    let temp = TempDir::new().expect("temp directory");
    let package = TestSigner::new().package(temp.path(), "1.0.0", ArchiveMutation::None);
    let source = package.install_source();
    let installer = PluginInstaller::new(temp.path().join("plugin-store"));
    let pending = installer
        .begin_network_install(&source.marketplace, &source.catalog, &source.release)
        .expect("begin network download transaction");
    let package_path = installer
        .plugin_root()
        .join(pending.relative_download_path.as_str());
    fs::create_dir_all(package_path.parent().expect("download parent"))
        .expect("create download parent");
    fs::copy(package.package_path(), package_path.as_path()).expect("stage downloaded artifact");

    let error = installer
        .install_downloaded_package(
            pending,
            PluginInstallRequest {
                marketplace: &source.marketplace,
                catalog: &source.catalog,
                release: &source.release,
                package_path: package_path.as_path(),
            },
        )
        .expect_err("uncommitted download progress must fail closed");
    assert!(error.to_string().contains("download progress"));
    let status = installer.status_snapshot().expect("rejected status");
    assert_eq!(
        status
            .transactions
            .history
            .last()
            .map(|record| record.status),
        Some(PluginInstallStatus::Rejected)
    );
}

#[test]
fn failed_or_interrupted_network_downloads_are_rejected_and_recovered() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
    let source = package.install_source();
    let installer = PluginInstaller::new(temp.path().join("plugin-store"));
    let rejected = installer
        .begin_network_install(&source.marketplace, &source.catalog, &source.release)
        .expect("begin rejected download");
    installer
        .reject_pending_install(&rejected, "proxy download failed")
        .expect("reject failed download");
    let status = installer.status_snapshot().expect("rejected status");
    assert_eq!(
        status
            .transactions
            .history
            .last()
            .map(|record| record.status),
        Some(PluginInstallStatus::Rejected)
    );

    let package = signer.package(temp.path(), "1.1.0", ArchiveMutation::None);
    let source = package.install_source();
    let interrupted = installer
        .begin_network_install(&source.marketplace, &source.catalog, &source.release)
        .expect("begin interrupted download");
    let download_path = installer
        .plugin_root()
        .join(interrupted.relative_download_path.as_str());
    fs::create_dir_all(download_path.parent().expect("download parent"))
        .expect("create download parent");
    fs::copy(package.package_path(), download_path.as_path()).expect("write interrupted download");
    let report = installer
        .recover_incomplete_transactions()
        .expect("recover interrupted download");
    assert_eq!(report.rolled_back_transactions, 1);
    assert!(!download_path.exists());
    let status = installer.status_snapshot().expect("recovered status");
    assert!(status.transactions.active.is_empty());
    assert_eq!(
        status
            .transactions
            .history
            .last()
            .map(|record| record.status),
        Some(PluginInstallStatus::Rejected)
    );
}

#[test]
fn updates_rollbacks_and_uninstalls_purge_release_scoped_credentials() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package_v1 = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
    let package_v2 = signer.package(temp.path(), "1.1.0", ArchiveMutation::None);
    let vault = PluginCredentialVault::in_memory(temp.path());
    let installer =
        PluginInstaller::new(temp.path().join("plugins")).with_credential_vault(vault.clone());

    let v1 = installer
        .install_package(package_v1.install_request())
        .expect("install v1");
    let component_key = v1.installed_version.inventory.components[0]
        .component_key
        .clone();
    let scope_v1 = PluginCredentialScope::new(
        "user-a",
        "device-a",
        PLUGIN_ID,
        v1.installed_version.release_id.as_str(),
        component_key.as_str(),
        "token",
    )
    .expect("v1 credential scope");
    vault.upsert(&scope_v1, b"v1-secret").expect("store v1");
    let v1_handle = vault
        .issue_handle(&scope_v1, Duration::from_secs(30))
        .expect("issue v1 handle");

    let v2 = installer
        .install_package(package_v2.install_request())
        .expect("update to v2");
    assert!(vault.resolve_handle(v1_handle.as_str(), &scope_v1).is_err());
    assert!(vault
        .list(
            "user-a",
            "device-a",
            PLUGIN_ID,
            v1.installed_version.release_id.as_str(),
        )
        .expect("list purged v1")
        .is_empty());

    let scope_v2 = PluginCredentialScope::new(
        "user-a",
        "device-a",
        PLUGIN_ID,
        v2.installed_version.release_id.as_str(),
        component_key.as_str(),
        "token",
    )
    .expect("v2 credential scope");
    vault.upsert(&scope_v2, b"v2-secret").expect("store v2");
    installer.rollback(PLUGIN_ID).expect("rollback to v1");
    assert!(vault
        .list(
            "user-a",
            "device-a",
            PLUGIN_ID,
            v2.installed_version.release_id.as_str(),
        )
        .expect("list purged v2")
        .is_empty());

    vault
        .upsert(&scope_v1, b"new-v1-secret")
        .expect("store new v1 secret");
    assert!(installer.uninstall(PLUGIN_ID).expect("uninstall Plugin"));
    assert!(vault
        .list(
            "user-a",
            "device-a",
            PLUGIN_ID,
            v1.installed_version.release_id.as_str(),
        )
        .expect("list after uninstall")
        .is_empty());
}

#[test]
fn restart_recovery_finishes_committed_installs_and_removes_orphans() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
    let installer = PluginInstaller::new(temp.path().join("plugin-store"));
    let installed = installer
        .install_package(package.install_request())
        .expect("install Plugin");

    let committed_id = "recovery-committed";
    let committed_staging = ".staging/recovery-committed";
    fs::create_dir_all(installer.plugin_root().join(committed_staging))
        .expect("create stale committed staging");
    begin_transaction(
        installer.plugin_root(),
        recovery_record(
            committed_id,
            PLUGIN_ID,
            Some("1.0.0"),
            Some(committed_staging),
            Some(
                installed
                    .installed_version
                    .relative_installation_path
                    .as_str(),
            ),
        ),
    )
    .expect("inject committed transaction");
    let committed_report = installer
        .recover_incomplete_transactions()
        .expect("recover committed transaction");
    assert_eq!(committed_report.completed_transactions, 1);
    assert!(!installer.plugin_root().join(committed_staging).exists());

    let orphan_id = "recovery-orphan";
    let orphan_staging = ".staging/recovery-orphan";
    let orphan_final = "installed/orphan-plugin/9.9.9";
    fs::create_dir_all(installer.plugin_root().join(orphan_staging))
        .expect("create orphan staging");
    fs::create_dir_all(installer.plugin_root().join(orphan_final))
        .expect("create orphan final version");
    begin_transaction(
        installer.plugin_root(),
        recovery_record(
            orphan_id,
            "orphan-plugin",
            Some("9.9.9"),
            Some(orphan_staging),
            Some(orphan_final),
        ),
    )
    .expect("inject orphan transaction");
    let orphan_report = installer
        .recover_incomplete_transactions()
        .expect("recover orphan transaction");
    assert_eq!(orphan_report.rolled_back_transactions, 1);
    assert!(!installer.plugin_root().join(orphan_staging).exists());
    assert!(!installer.plugin_root().join(orphan_final).exists());

    let snapshot = installer.status_snapshot().expect("recovered status");
    assert!(snapshot.transactions.active.is_empty());
    assert!(snapshot.transactions.history.iter().any(|record| {
        record.transaction_id == committed_id
            && record.recovered_after_restart
            && record.status == PluginInstallStatus::Installed
    }));
    assert!(snapshot.transactions.history.iter().any(|record| {
        record.transaction_id == orphan_id
            && record.recovered_after_restart
            && record.status == PluginInstallStatus::Rejected
    }));
}

#[test]
fn rejects_artifact_tampering_before_extraction() {
    let temp = TempDir::new().expect("temp directory");
    let signer = TestSigner::new();
    let package = signer.package(temp.path(), "1.0.0", ArchiveMutation::None);
    OpenOptions::new()
        .append(true)
        .open(&package.package_path)
        .expect("open artifact")
        .write_all(b"tamper")
        .expect("tamper artifact");
    let extraction = temp.path().join("extracted");
    let error = verify_plugin_package(
        package.verification_request(extraction.as_path()),
        PluginPackageLimits::default(),
    )
    .expect_err("tampered artifact must fail");
    assert!(
        error.to_string().contains("npm integrity"),
        "{error:#}"
    );
    assert!(!extraction.exists());
}

#[test]
fn rejects_unsafe_npm_package_symlinks_and_duplicates() {
    for (label, mutation) in [
        ("symlink", ArchiveMutation::Symlink),
        ("duplicate", ArchiveMutation::Duplicate),
    ] {
        let temp = TempDir::new().expect("temp directory");
        let signer = TestSigner::new();
        let package = signer.package(temp.path(), "1.0.0", mutation);
        let extraction = temp.path().join(format!("extract-{label}"));
        let error = verify_plugin_package(
            package.verification_request(extraction.as_path()),
            PluginPackageLimits::default(),
        )
        .expect_err("unsafe npm package must fail");
        assert!(
            error
                .to_string()
                .contains("symlink, hard link, or special file")
                || error.to_string().contains("duplicate or case-colliding")
        );
        assert!(!extraction.exists());
    }
}

#[test]
fn rejects_missing_package_identity_or_wrong_npm_integrity() {
    for (mutation, expected) in [
        (ArchiveMutation::MissingPackageJson, "missing package.json"),
        (ArchiveMutation::WrongIntegrity, "npm integrity"),
    ] {
        let temp = TempDir::new().expect("temp directory");
        let signer = TestSigner::new();
        let package = signer.package(temp.path(), "1.0.0", mutation);
        let extraction = temp.path().join("extract-checksums");
        let error = verify_plugin_package(
            package.verification_request(extraction.as_path()),
            PluginPackageLimits::default(),
        )
        .expect_err("invalid npm package must fail");
        assert!(error.to_string().contains(expected), "{error:#}");
        assert!(!extraction.exists());
    }
}
