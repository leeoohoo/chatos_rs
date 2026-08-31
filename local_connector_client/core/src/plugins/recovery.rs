// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chatos_plugin_management_sdk::PluginInstallStatus;
use chrono::Utc;

use super::archive::{verify_installed_file_checksums, PluginPackageLimits};
use super::journal::{
    finish_transaction, load_journal, PluginRecoveryReport, PluginTransactionOperation,
    PluginTransactionRecord,
};
use super::state::load_registry;

pub(super) fn recover_incomplete_transactions(
    plugin_root: &Path,
    limits: PluginPackageLimits,
) -> Result<PluginRecoveryReport> {
    let journal = load_journal(plugin_root)?;
    let registry = load_registry(plugin_root)?;
    let mut report = PluginRecoveryReport::default();
    for record in journal.active.values().cloned().collect::<Vec<_>>() {
        let result = recover_transaction(plugin_root, &registry, &record, limits);
        match result {
            Ok((status, completed, error)) => {
                if completed {
                    report.completed_transactions += 1;
                } else {
                    report.rolled_back_transactions += 1;
                }
                if let Err(finish_error) = finish_transaction(
                    plugin_root,
                    record.transaction_id.as_str(),
                    status,
                    Utc::now().to_rfc3339(),
                    true,
                    error,
                ) {
                    report.errors.push(format!(
                        "finish recovered transaction {} failed: {finish_error}",
                        record.transaction_id
                    ));
                }
            }
            Err(error) => report.errors.push(format!(
                "recover transaction {} failed: {error}",
                record.transaction_id
            )),
        }
    }
    report.cleaned_paths += cleanup_unreferenced_work_paths(plugin_root)?;
    Ok(report)
}

fn recover_transaction(
    plugin_root: &Path,
    registry: &super::LocalPluginRegistry,
    record: &PluginTransactionRecord,
    limits: PluginPackageLimits,
) -> Result<(PluginInstallStatus, bool, Option<String>)> {
    match record.operation {
        PluginTransactionOperation::Install | PluginTransactionOperation::Update => {
            recover_install(plugin_root, registry, record, limits)
        }
        PluginTransactionOperation::Rollback => Ok(recover_rollback(registry, record)),
        PluginTransactionOperation::Uninstall => recover_uninstall(plugin_root, registry, record),
    }
}

fn recover_install(
    plugin_root: &Path,
    registry: &super::LocalPluginRegistry,
    record: &PluginTransactionRecord,
    limits: PluginPackageLimits,
) -> Result<(PluginInstallStatus, bool, Option<String>)> {
    remove_record_path(plugin_root, record.relative_storage_path.as_deref())?;
    remove_record_path(plugin_root, record.relative_staging_path.as_deref())?;
    let target = record.target_version.as_deref().unwrap_or_default();
    let registered = registry
        .plugins
        .get(record.plugin_id.as_str())
        .and_then(|plugin| plugin.versions.get(target));
    if let Some(version) = registered {
        let path = plugin_root.join(version.relative_installation_path.as_str());
        verify_installed_file_checksums(path.as_path(), &version.package_file_sha256, limits)?;
        let active = registry
            .plugins
            .get(record.plugin_id.as_str())
            .and_then(|plugin| plugin.active_version.as_deref())
            == Some(target);
        if active {
            return Ok((PluginInstallStatus::Installed, true, None));
        }
        return Ok((
            PluginInstallStatus::Rejected,
            false,
            Some("interrupted install committed a non-active version".to_string()),
        ));
    }
    remove_record_path(plugin_root, record.relative_final_path.as_deref())?;
    Ok((
        PluginInstallStatus::Rejected,
        false,
        Some("interrupted install was rolled back before activation".to_string()),
    ))
}

fn recover_rollback(
    registry: &super::LocalPluginRegistry,
    record: &PluginTransactionRecord,
) -> (PluginInstallStatus, bool, Option<String>) {
    let active = registry
        .plugins
        .get(record.plugin_id.as_str())
        .and_then(|plugin| plugin.active_version.as_deref());
    if active == record.target_version.as_deref() {
        (PluginInstallStatus::Installed, true, None)
    } else {
        (
            PluginInstallStatus::Rejected,
            false,
            Some("interrupted rollback did not commit the target version".to_string()),
        )
    }
}

fn recover_uninstall(
    plugin_root: &Path,
    registry: &super::LocalPluginRegistry,
    record: &PluginTransactionRecord,
) -> Result<(PluginInstallStatus, bool, Option<String>)> {
    let trash = record
        .relative_trash_path
        .as_deref()
        .map(|path| plugin_root.join(path));
    if !registry.plugins.contains_key(record.plugin_id.as_str()) {
        if let Some(trash) = trash {
            remove_path(trash.as_path())?;
        }
        return Ok((PluginInstallStatus::NotInstalled, true, None));
    }
    if let (Some(storage), Some(trash)) = (
        record
            .relative_storage_path
            .as_deref()
            .map(|path| plugin_root.join(path)),
        trash,
    ) {
        if trash.exists() && !storage.exists() {
            if let Some(parent) = storage.parent() {
                fs::create_dir_all(parent).context("restore Plugin storage parent")?;
            }
            fs::rename(trash, storage).context("restore interrupted Plugin uninstall")?;
        }
    }
    Ok((
        PluginInstallStatus::Rejected,
        false,
        Some("interrupted uninstall was rolled back".to_string()),
    ))
}

fn cleanup_unreferenced_work_paths(plugin_root: &Path) -> Result<usize> {
    let active = load_journal(plugin_root)?
        .active
        .values()
        .flat_map(|record| {
            [
                record.relative_staging_path.clone(),
                record.relative_storage_path.clone(),
                record.relative_trash_path.clone(),
            ]
        })
        .flatten()
        .collect::<HashSet<_>>();
    let mut cleaned = 0;
    for directory in [".downloads", ".staging", ".trash"] {
        let root = plugin_root.join(directory);
        if !root.exists() {
            continue;
        }
        for entry in fs::read_dir(&root).context("read Plugin recovery work directory")? {
            let entry = entry.context("read Plugin recovery work entry")?;
            let relative = entry
                .path()
                .strip_prefix(plugin_root)
                .context("derive Plugin recovery work path")?
                .to_string_lossy()
                .replace('\\', "/");
            if !active.contains(relative.as_str()) {
                remove_path(entry.path().as_path())?;
                cleaned += 1;
            }
        }
    }
    Ok(cleaned)
}

fn remove_record_path(plugin_root: &Path, relative: Option<&str>) -> Result<()> {
    if let Some(relative) = relative {
        remove_path(plugin_root.join(relative).as_path())?;
    }
    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
            .with_context(|| format!("remove Plugin recovery directory: {}", path.display()))
    } else {
        fs::remove_file(path)
            .with_context(|| format!("remove Plugin recovery file: {}", path.display()))
    }
}
