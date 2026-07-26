// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::PluginInstallStatus;
use chrono::Utc;
use semver::Version;
use uuid::Uuid;

use super::archive::verify_installed_file_checksums;
use super::installer::{
    plugin_storage_key, ActivePluginInstallation, PluginInstallOutcome, PluginInstallRequest,
    PluginInstaller,
};
use super::journal::{
    begin_transaction, finish_transaction, PluginTransactionOperation, PluginTransactionRecord,
};
use super::state::{
    save_registry, InstalledPluginVersion, LocalInstalledPlugin, LocalPluginRegistry,
};

impl PluginInstaller {
    pub fn active_installation(&self, plugin_id: &str) -> Result<Option<ActivePluginInstallation>> {
        let registry = self.registry()?;
        let Some(plugin) = registry.plugins.get(plugin_id) else {
            return Ok(None);
        };
        let Some(active_version) = plugin.active_version.as_deref() else {
            return Ok(None);
        };
        let version = plugin
            .versions
            .get(active_version)
            .context("active Plugin version is missing from the registry")?
            .clone();
        let installation_path = self.plugin_root.join(&version.relative_installation_path);
        verify_installed_file_checksums(
            installation_path.as_path(),
            &version.package_file_sha256,
            self.limits,
        )?;
        Ok(Some(ActivePluginInstallation {
            plugin_id: plugin_id.to_string(),
            version,
            installation_path,
        }))
    }

    pub fn rollback(&self, plugin_id: &str) -> Result<ActivePluginInstallation> {
        let _guard = self.operation_guard()?;
        let mut registry = self.registry()?;
        let plugin = registry
            .plugins
            .get_mut(plugin_id)
            .context("Plugin is not installed")?;
        let current = plugin
            .active_version
            .clone()
            .context("Plugin has no active version")?;
        let current_release_id = plugin
            .versions
            .get(current.as_str())
            .context("active Plugin version is missing from the registry")?
            .release_id
            .clone();
        let target = plugin
            .previous_version
            .clone()
            .context("Plugin has no verified rollback target")?;
        let target_record = plugin
            .versions
            .get(target.as_str())
            .context("Plugin rollback target is not installed")?;
        let transaction_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        begin_transaction(
            self.plugin_root.as_path(),
            PluginTransactionRecord {
                transaction_id: transaction_id.clone(),
                operation: PluginTransactionOperation::Rollback,
                status: PluginInstallStatus::RollingBack,
                plugin_id: plugin_id.to_string(),
                release_id: Some(target_record.release_id.clone()),
                from_version: Some(current.clone()),
                target_version: Some(target.clone()),
                relative_staging_path: None,
                relative_final_path: Some(target_record.relative_installation_path.clone()),
                relative_storage_path: None,
                relative_trash_path: None,
                downloaded_bytes: 0,
                total_bytes: None,
                started_at: now.clone(),
                updated_at: now,
                completed_at: None,
                recovered_after_restart: false,
                last_error: None,
            },
        )?;
        let result = self.rollback_inner(plugin_id, registry, current, target);
        let installation = self.finish_operation(
            transaction_id.as_str(),
            result,
            PluginInstallStatus::Installed,
        )?;
        self.purge_release_credentials(plugin_id, current_release_id.as_str())
            .context("Plugin rollback committed, but replaced Release credential cleanup failed")?;
        Ok(installation)
    }

    pub fn uninstall(&self, plugin_id: &str) -> Result<bool> {
        let _guard = self.operation_guard()?;
        let mut registry = self.registry()?;
        let Some(plugin) = registry.plugins.remove(plugin_id) else {
            return Ok(false);
        };
        let storage_path = self.plugin_storage_path(plugin_id, plugin.plugin_name.as_str());
        let transaction_id = Uuid::new_v4().to_string();
        let relative_storage_path = storage_path
            .strip_prefix(self.plugin_root.as_path())
            .context("derive Plugin storage path")?
            .to_string_lossy()
            .replace('\\', "/");
        let relative_trash_path = format!(".trash/{transaction_id}");
        let now = Utc::now().to_rfc3339();
        begin_transaction(
            self.plugin_root.as_path(),
            PluginTransactionRecord {
                transaction_id: transaction_id.clone(),
                operation: PluginTransactionOperation::Uninstall,
                status: PluginInstallStatus::Uninstalling,
                plugin_id: plugin_id.to_string(),
                release_id: None,
                from_version: plugin.active_version.clone(),
                target_version: None,
                relative_staging_path: None,
                relative_final_path: None,
                relative_storage_path: Some(relative_storage_path),
                relative_trash_path: Some(relative_trash_path.clone()),
                downloaded_bytes: 0,
                total_bytes: None,
                started_at: now.clone(),
                updated_at: now,
                completed_at: None,
                recovered_after_restart: false,
                last_error: None,
            },
        )?;
        let result = (|| {
            self.purge_plugin_credentials(plugin_id)
                .context("purge Plugin credentials before uninstall")?;
            self.uninstall_inner(
                registry,
                storage_path,
                self.plugin_root.join(relative_trash_path),
            )
        })();
        self.finish_operation(
            transaction_id.as_str(),
            result,
            PluginInstallStatus::NotInstalled,
        )
    }

    pub(super) fn activate_installed_version(
        &self,
        request: PluginInstallRequest<'_>,
        installed_version: InstalledPluginVersion,
        final_path: &Path,
    ) -> Result<PluginInstallOutcome> {
        self.activate_verified_version(
            request.catalog.id.as_str(),
            request.marketplace.id.as_str(),
            request.catalog.name.as_str(),
            installed_version,
            final_path,
        )
    }

    pub(super) fn activate_verified_version(
        &self,
        plugin_id: &str,
        marketplace_id: &str,
        plugin_name: &str,
        installed_version: InstalledPluginVersion,
        final_path: &Path,
    ) -> Result<PluginInstallOutcome> {
        let mut registry = self.registry()?;
        let plugin = registry
            .plugins
            .entry(plugin_id.to_string())
            .or_insert_with(|| LocalInstalledPlugin {
                plugin_id: plugin_id.to_string(),
                marketplace_id: marketplace_id.to_string(),
                plugin_name: plugin_name.to_string(),
                active_version: None,
                previous_version: None,
                versions: BTreeMap::new(),
            });
        if plugin.marketplace_id != marketplace_id
            || plugin.plugin_name != plugin_name
            || plugin
                .versions
                .contains_key(installed_version.version.as_str())
        {
            let _ = fs::remove_dir_all(final_path);
            bail!("local Plugin registry conflicts with the verified Release identity");
        }
        let previous = plugin.active_version.clone();
        plugin
            .versions
            .insert(installed_version.version.clone(), installed_version.clone());
        plugin.active_version = Some(installed_version.version.clone());
        plugin.previous_version = previous;
        let plugin_snapshot = plugin.clone();
        if let Err(error) = save_registry(self.plugin_root.as_path(), &registry) {
            let _ = fs::remove_dir_all(final_path);
            return Err(error);
        }
        Ok(PluginInstallOutcome {
            plugin: plugin_snapshot,
            installed_version,
            installation_path: final_path.to_path_buf(),
        })
    }

    pub(super) fn ensure_upgrade_is_allowed(
        &self,
        plugin_id: &str,
        next_version: &str,
    ) -> Result<()> {
        let registry = self.registry()?;
        let Some(plugin) = registry.plugins.get(plugin_id) else {
            return Ok(());
        };
        let Some(active) = plugin.active_version.as_deref() else {
            return Ok(());
        };
        let active = Version::parse(active).context("active Plugin version is invalid")?;
        let next = Version::parse(next_version).context("Plugin Release version is invalid")?;
        if next <= active {
            bail!("Plugin installation would downgrade or replace the active immutable version");
        }
        Ok(())
    }

    pub(super) fn finish_operation<T>(
        &self,
        transaction_id: &str,
        result: Result<T>,
        success_status: PluginInstallStatus,
    ) -> Result<T> {
        match result {
            Ok(value) => {
                finish_transaction(
                    self.plugin_root.as_path(),
                    transaction_id,
                    success_status,
                    Utc::now().to_rfc3339(),
                    false,
                    None,
                )?;
                Ok(value)
            }
            Err(error) => {
                if let Err(journal_error) = finish_transaction(
                    self.plugin_root.as_path(),
                    transaction_id,
                    PluginInstallStatus::Rejected,
                    Utc::now().to_rfc3339(),
                    false,
                    Some(error.to_string()),
                ) {
                    return Err(error.context(format!(
                        "persist rejected Plugin transaction failed: {journal_error}"
                    )));
                }
                Err(error)
            }
        }
    }

    pub(super) fn operation_guard(&self) -> Result<MutexGuard<'_, ()>> {
        self.operation_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin installer operation lock is poisoned"))
    }

    pub(super) fn relative_installation_path(
        &self,
        plugin_id: &str,
        plugin_name: &str,
        version: &str,
    ) -> String {
        format!(
            "installed/{}/{}",
            plugin_storage_key(plugin_id, plugin_name),
            version
        )
    }

    fn plugin_storage_path(&self, plugin_id: &str, plugin_name: &str) -> PathBuf {
        self.plugin_root
            .join("installed")
            .join(plugin_storage_key(plugin_id, plugin_name))
    }

    fn rollback_inner(
        &self,
        plugin_id: &str,
        mut registry: LocalPluginRegistry,
        current: String,
        target: String,
    ) -> Result<ActivePluginInstallation> {
        let plugin = registry
            .plugins
            .get_mut(plugin_id)
            .context("Plugin is not installed")?;
        let target_version = plugin
            .versions
            .get(target.as_str())
            .context("Plugin rollback target is not installed")?
            .clone();
        let target_path = self
            .plugin_root
            .join(target_version.relative_installation_path.as_str());
        verify_installed_file_checksums(
            target_path.as_path(),
            &target_version.package_file_sha256,
            self.limits,
        )?;
        plugin.active_version = Some(target);
        plugin.previous_version = Some(current);
        save_registry(self.plugin_root.as_path(), &registry)?;
        Ok(ActivePluginInstallation {
            plugin_id: plugin_id.to_string(),
            version: target_version,
            installation_path: target_path,
        })
    }

    fn uninstall_inner(
        &self,
        registry: LocalPluginRegistry,
        storage_path: PathBuf,
        trash_path: PathBuf,
    ) -> Result<bool> {
        if let Some(parent) = trash_path.parent() {
            fs::create_dir_all(parent).context("create Plugin uninstall trash directory")?;
        }
        let moved = if storage_path.exists() {
            fs::rename(storage_path.as_path(), trash_path.as_path())
                .context("atomically detach Plugin installation before uninstall")?;
            true
        } else {
            false
        };
        if let Err(error) = save_registry(self.plugin_root.as_path(), &registry) {
            if moved {
                let _ = fs::rename(trash_path.as_path(), storage_path.as_path());
            }
            return Err(error);
        }
        if moved {
            let _ = fs::remove_dir_all(trash_path);
        }
        Ok(true)
    }
}
