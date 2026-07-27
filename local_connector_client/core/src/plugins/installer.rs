// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, PluginCatalogRecord, PluginInstallStatus,
    PluginMarketplaceRecord, PluginReleaseRecord,
};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::archive::PluginArchiveLimits;
use super::credentials::PluginCredentialVault;
use super::journal::{
    begin_transaction, finish_transaction, load_journal, transition_transaction,
    update_download_progress, LocalPluginStatusSnapshot, PluginRecoveryReport,
    PluginTransactionOperation, PluginTransactionRecord,
};
use super::recovery::recover_incomplete_transactions;
use super::state::{
    load_registry, InstalledPluginVersion, LocalInstalledPlugin, LocalPluginRegistry,
};
use super::verifier::{
    verify_plugin_artifact, verify_plugin_install_source_records, PluginArtifactVerificationRequest,
};

const INSTALLATION_METADATA_PATH: &str = ".chatos-installation.json";

#[derive(Debug, Clone, Copy)]
pub struct PluginInstallRequest<'a> {
    pub marketplace: &'a PluginMarketplaceRecord,
    pub catalog: &'a PluginCatalogRecord,
    pub release: &'a PluginReleaseRecord,
    pub archive_path: &'a Path,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginInstallOutcome {
    pub plugin: LocalInstalledPlugin,
    pub installed_version: InstalledPluginVersion,
    pub installation_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingPluginInstall {
    pub(crate) transaction_id: String,
    pub(crate) plugin_id: String,
    pub(crate) release_id: String,
    pub(crate) operation: PluginTransactionOperation,
    pub(crate) relative_staging_path: String,
    pub(crate) relative_final_path: String,
    pub(crate) relative_download_path: String,
    pub(crate) previous_release_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivePluginInstallation {
    pub plugin_id: String,
    pub version: InstalledPluginVersion,
    pub installation_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PluginInstaller {
    pub(super) plugin_root: PathBuf,
    pub(super) limits: PluginArchiveLimits,
    pub(super) operation_lock: Arc<Mutex<()>>,
    pub(super) credential_vault: Option<PluginCredentialVault>,
}

impl PluginInstaller {
    pub fn new(plugin_root: PathBuf) -> Self {
        Self {
            plugin_root,
            limits: PluginArchiveLimits::default(),
            operation_lock: Arc::new(Mutex::new(())),
            credential_vault: None,
        }
    }

    pub fn for_state_path(state_path: &Path) -> Self {
        let app_data = state_path.parent().unwrap_or_else(|| Path::new("."));
        Self::new(app_data.join("plugins"))
    }

    pub fn with_limits(mut self, limits: PluginArchiveLimits) -> Self {
        self.limits = limits;
        self
    }

    pub(crate) fn with_credential_vault(mut self, credential_vault: PluginCredentialVault) -> Self {
        self.credential_vault = Some(credential_vault);
        self
    }

    pub fn plugin_root(&self) -> &Path {
        self.plugin_root.as_path()
    }

    pub fn archive_limits(&self) -> PluginArchiveLimits {
        self.limits
    }

    pub(crate) fn credential_vault(&self) -> Option<PluginCredentialVault> {
        self.credential_vault.clone()
    }

    pub fn registry(&self) -> Result<LocalPluginRegistry> {
        load_registry(self.plugin_root.as_path())
    }

    pub fn status_snapshot(&self) -> Result<LocalPluginStatusSnapshot> {
        Ok(LocalPluginStatusSnapshot {
            registry: self.registry()?,
            transactions: load_journal(self.plugin_root.as_path())?,
            runtime: Default::default(),
        })
    }

    pub fn recover_incomplete_transactions(&self) -> Result<PluginRecoveryReport> {
        let _guard = self.operation_guard()?;
        recover_incomplete_transactions(self.plugin_root.as_path(), self.limits)
    }

    pub fn install_archive(
        &self,
        request: PluginInstallRequest<'_>,
    ) -> Result<PluginInstallOutcome> {
        let _guard = self.operation_guard()?;
        self.ensure_upgrade_is_allowed(
            request.catalog.id.as_str(),
            request.release.version.as_str(),
        )?;
        let registry = self.registry()?;
        let from_version = registry
            .plugins
            .get(request.catalog.id.as_str())
            .and_then(|plugin| plugin.active_version.clone());
        let from_release_id = registry
            .plugins
            .get(request.catalog.id.as_str())
            .and_then(|plugin| {
                plugin
                    .active_version
                    .as_deref()
                    .and_then(|version| plugin.versions.get(version))
            })
            .map(|version| version.release_id.clone());
        let operation = if from_version.is_some() {
            PluginTransactionOperation::Update
        } else {
            PluginTransactionOperation::Install
        };
        let transaction_id = Uuid::new_v4().to_string();
        let relative_staging_path = format!(".staging/install-{transaction_id}");
        let relative_final_path = self.relative_installation_path(
            request.catalog.id.as_str(),
            request.catalog.name.as_str(),
            request.release.version.as_str(),
        );
        let now = Utc::now().to_rfc3339();
        begin_transaction(
            self.plugin_root.as_path(),
            PluginTransactionRecord {
                transaction_id: transaction_id.clone(),
                operation,
                status: PluginInstallStatus::Verifying,
                plugin_id: request.catalog.id.clone(),
                release_id: Some(request.release.id.clone()),
                from_version,
                target_version: Some(request.release.version.clone()),
                relative_staging_path: Some(relative_staging_path.clone()),
                relative_final_path: Some(relative_final_path.clone()),
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
        let result = self.install_archive_inner(
            request,
            transaction_id.as_str(),
            operation,
            relative_staging_path.as_str(),
            relative_final_path,
        );
        let outcome = self.finish_operation(
            transaction_id.as_str(),
            result,
            PluginInstallStatus::Installed,
        )?;
        if let Some(release_id) = from_release_id {
            self.purge_release_credentials(request.catalog.id.as_str(), release_id.as_str())
                .context(
                    "Plugin update committed, but previous Release credential cleanup failed",
                )?;
        }
        Ok(outcome)
    }

    pub(crate) fn begin_network_install(
        &self,
        marketplace: &PluginMarketplaceRecord,
        catalog: &PluginCatalogRecord,
        release: &PluginReleaseRecord,
    ) -> Result<PendingPluginInstall> {
        let _guard = self.operation_guard()?;
        verify_plugin_install_source_records(marketplace, catalog, release)?;
        if catalog.latest_release_id != release.id || release.release_channel != "stable" {
            bail!("network Plugin install source is not the current stable Release");
        }
        self.ensure_upgrade_is_allowed(catalog.id.as_str(), release.version.as_str())?;
        let registry = self.registry()?;
        let from_version = registry
            .plugins
            .get(catalog.id.as_str())
            .and_then(|plugin| plugin.active_version.clone());
        let previous_release_id = registry
            .plugins
            .get(catalog.id.as_str())
            .and_then(|plugin| {
                plugin
                    .active_version
                    .as_deref()
                    .and_then(|version| plugin.versions.get(version))
            })
            .map(|version| version.release_id.clone());
        let operation = if from_version.is_some() {
            PluginTransactionOperation::Update
        } else {
            PluginTransactionOperation::Install
        };
        let transaction_id = Uuid::new_v4().to_string();
        let relative_staging_path = format!(".staging/install-{transaction_id}");
        let relative_download_path = format!(".downloads/{transaction_id}.zip");
        let relative_final_path = self.relative_installation_path(
            catalog.id.as_str(),
            catalog.name.as_str(),
            release.version.as_str(),
        );
        let now = Utc::now().to_rfc3339();
        begin_transaction(
            self.plugin_root.as_path(),
            PluginTransactionRecord {
                transaction_id: transaction_id.clone(),
                operation,
                status: PluginInstallStatus::Downloading,
                plugin_id: catalog.id.clone(),
                release_id: Some(release.id.clone()),
                from_version,
                target_version: Some(release.version.clone()),
                relative_staging_path: Some(relative_staging_path.clone()),
                relative_final_path: Some(relative_final_path.clone()),
                relative_storage_path: Some(relative_download_path.clone()),
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
        Ok(PendingPluginInstall {
            transaction_id,
            plugin_id: catalog.id.clone(),
            release_id: release.id.clone(),
            operation,
            relative_staging_path,
            relative_final_path,
            relative_download_path,
            previous_release_id,
        })
    }

    pub(crate) fn update_network_download_progress(
        &self,
        pending: &PendingPluginInstall,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<()> {
        let _guard = self.operation_guard()?;
        if downloaded_bytes > self.limits.max_archive_bytes
            || total_bytes.is_some_and(|total| total > self.limits.max_archive_bytes)
        {
            bail!("Plugin download progress exceeds the archive size limit");
        }
        let journal = load_journal(self.plugin_root.as_path())?;
        let record = journal
            .active
            .get(pending.transaction_id.as_str())
            .context("Plugin download transaction is not active")?;
        if record.plugin_id != pending.plugin_id
            || record.release_id.as_deref() != Some(pending.release_id.as_str())
            || record.status != PluginInstallStatus::Downloading
            || record.relative_storage_path.as_deref()
                != Some(pending.relative_download_path.as_str())
        {
            bail!("Plugin download transaction identity or state changed");
        }
        update_download_progress(
            self.plugin_root.as_path(),
            pending.transaction_id.as_str(),
            downloaded_bytes,
            total_bytes,
            Utc::now().to_rfc3339(),
        )
    }

    pub(crate) fn reject_pending_install(
        &self,
        pending: &PendingPluginInstall,
        error: &str,
    ) -> Result<()> {
        let _guard = self.operation_guard()?;
        let journal = load_journal(self.plugin_root.as_path())?;
        let record = journal
            .active
            .get(pending.transaction_id.as_str())
            .context("Plugin download transaction is not active")?;
        if record.plugin_id != pending.plugin_id
            || record.release_id.as_deref() != Some(pending.release_id.as_str())
            || record.status != PluginInstallStatus::Downloading
        {
            bail!("Plugin download transaction identity or state changed");
        }
        finish_transaction(
            self.plugin_root.as_path(),
            pending.transaction_id.as_str(),
            PluginInstallStatus::Rejected,
            Utc::now().to_rfc3339(),
            false,
            Some(error.chars().take(1_000).collect()),
        )
    }

    pub(crate) fn install_downloaded_archive(
        &self,
        pending: PendingPluginInstall,
        request: PluginInstallRequest<'_>,
    ) -> Result<PluginInstallOutcome> {
        let _guard = self.operation_guard()?;
        let result = (|| {
            if pending.plugin_id != request.catalog.id
                || pending.release_id != request.release.id
                || pending.relative_download_path
                    != request
                        .archive_path
                        .strip_prefix(self.plugin_root.as_path())
                        .ok()
                        .map(|path| path.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default()
            {
                bail!("downloaded Plugin transaction does not match the install request");
            }
            let journal = load_journal(self.plugin_root.as_path())?;
            let record = journal
                .active
                .get(pending.transaction_id.as_str())
                .context("Plugin download transaction is not active")?;
            if record.plugin_id != pending.plugin_id
                || record.release_id.as_deref() != Some(pending.release_id.as_str())
                || record.status != PluginInstallStatus::Downloading
                || record.relative_storage_path.as_deref()
                    != Some(pending.relative_download_path.as_str())
            {
                bail!("Plugin download transaction identity or state changed");
            }
            let archive_bytes = fs::metadata(request.archive_path)
                .context("read downloaded Plugin artifact metadata")?
                .len();
            if record.downloaded_bytes != archive_bytes || record.total_bytes != Some(archive_bytes)
            {
                bail!("Plugin download progress does not match the completed artifact");
            }
            transition_transaction(
                self.plugin_root.as_path(),
                pending.transaction_id.as_str(),
                PluginInstallStatus::Verifying,
                Utc::now().to_rfc3339(),
            )?;
            self.install_archive_inner(
                request,
                pending.transaction_id.as_str(),
                pending.operation,
                pending.relative_staging_path.as_str(),
                pending.relative_final_path.clone(),
            )
        })();
        let outcome = self.finish_operation(
            pending.transaction_id.as_str(),
            result,
            PluginInstallStatus::Installed,
        )?;
        if let Some(release_id) = pending.previous_release_id {
            self.purge_release_credentials(request.catalog.id.as_str(), release_id.as_str())
                .context(
                    "Plugin update committed, but previous Release credential cleanup failed",
                )?;
        }
        Ok(outcome)
    }

    fn install_archive_inner(
        &self,
        request: PluginInstallRequest<'_>,
        transaction_id: &str,
        operation: PluginTransactionOperation,
        relative_staging_path: &str,
        relative_path: String,
    ) -> Result<PluginInstallOutcome> {
        let transaction =
            StagingTransaction::new(self.create_transaction_directory(relative_staging_path)?);
        let extraction_root = transaction.root().join("payload");
        let verification = verify_plugin_artifact(
            PluginArtifactVerificationRequest {
                marketplace: request.marketplace,
                catalog: request.catalog,
                release: request.release,
                archive_path: request.archive_path,
                extraction_root: extraction_root.as_path(),
            },
            self.limits,
        );
        let verified = verification?;
        let next_status = match operation {
            PluginTransactionOperation::Install => PluginInstallStatus::Installing,
            PluginTransactionOperation::Update => PluginInstallStatus::Updating,
            PluginTransactionOperation::Rollback | PluginTransactionOperation::Uninstall => {
                bail!("invalid Plugin install transaction operation")
            }
        };
        transition_transaction(
            self.plugin_root.as_path(),
            transaction_id,
            next_status,
            Utc::now().to_rfc3339(),
        )?;
        let final_path = self.plugin_root.join(relative_path.as_str());
        if final_path.exists() {
            bail!(
                "immutable Plugin version is already installed: {}",
                request.release.version
            );
        }
        let installed_version = InstalledPluginVersion {
            release_id: request.release.id.clone(),
            version: request.release.version.clone(),
            artifact_sha256: verified.artifact_sha256,
            manifest_sha256: normalized_plugin_manifest_sha256(&verified.manifest)
                .context("hash installed Plugin Manifest")?,
            signature_key_id: request.release.signature.key_id.clone(),
            relative_installation_path: relative_path.clone(),
            installed_at: Utc::now().to_rfc3339(),
            package_file_sha256: verified.package_file_sha256,
            inventory: verified.inventory,
        };
        write_installation_metadata(verified.root.as_path(), &installed_version)?;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create immutable Plugin version parent: {}",
                    parent.display()
                )
            })?;
        }
        fs::rename(verified.root.as_path(), final_path.as_path()).with_context(|| {
            format!(
                "atomically move verified Plugin into immutable storage: {}",
                final_path.display()
            )
        })?;

        self.activate_installed_version(request, installed_version.clone(), final_path.as_path())
    }

    fn create_transaction_directory(&self, relative_path: &str) -> Result<PathBuf> {
        let transaction = self.plugin_root.join(relative_path);
        let staging_root = transaction
            .parent()
            .context("Plugin staging transaction has no parent")?;
        fs::create_dir_all(staging_root).context("create Plugin staging directory")?;
        fs::create_dir(&transaction).context("create isolated Plugin installation transaction")?;
        Ok(transaction)
    }

    pub(super) fn purge_plugin_credentials(&self, plugin_id: &str) -> Result<usize> {
        self.credential_vault
            .as_ref()
            .map(|vault| vault.purge_plugin(plugin_id))
            .transpose()
            .map(|purged| purged.unwrap_or_default())
    }

    pub(super) fn purge_release_credentials(
        &self,
        plugin_id: &str,
        release_id: &str,
    ) -> Result<usize> {
        self.credential_vault
            .as_ref()
            .map(|vault| vault.purge_release(plugin_id, release_id))
            .transpose()
            .map(|purged| purged.unwrap_or_default())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallationMetadata<'a> {
    schema_version: u32,
    installation: &'a InstalledPluginVersion,
}

pub(super) fn write_installation_metadata(
    root: &Path,
    version: &InstalledPluginVersion,
) -> Result<()> {
    let path = root.join(INSTALLATION_METADATA_PATH);
    let payload = serde_json::to_vec_pretty(&InstallationMetadata {
        schema_version: 1,
        installation: version,
    })
    .context("serialize Plugin installation metadata")?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .context("create Plugin installation metadata")?;
    file.write_all(payload.as_slice())
        .context("write Plugin installation metadata")?;
    file.sync_all().context("sync Plugin installation metadata")
}

pub(super) fn plugin_storage_key(plugin_id: &str, plugin_name: &str) -> String {
    let digest = hex::encode(Sha256::digest(plugin_id.as_bytes()));
    format!("{plugin_name}--{}", &digest[..16])
}

struct StagingTransaction {
    root: PathBuf,
}

impl StagingTransaction {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn root(&self) -> &Path {
        self.root.as_path()
    }
}

impl Drop for StagingTransaction {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
