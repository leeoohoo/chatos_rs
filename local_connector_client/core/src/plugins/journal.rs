// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::PluginInstallStatus;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::{LocalPluginRegistry, PluginRuntimeTelemetrySnapshot};

const JOURNAL_SCHEMA_VERSION: u32 = 1;
const MAX_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
const MAX_HISTORY_ENTRIES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginTransactionOperation {
    Install,
    Update,
    Rollback,
    Uninstall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTransactionRecord {
    pub transaction_id: String,
    pub operation: PluginTransactionOperation,
    pub status: PluginInstallStatus,
    pub plugin_id: String,
    #[serde(default)]
    pub release_id: Option<String>,
    #[serde(default)]
    pub from_version: Option<String>,
    #[serde(default)]
    pub target_version: Option<String>,
    #[serde(default)]
    pub relative_staging_path: Option<String>,
    #[serde(default)]
    pub relative_final_path: Option<String>,
    #[serde(default)]
    pub relative_storage_path: Option<String>,
    #[serde(default)]
    pub relative_trash_path: Option<String>,
    #[serde(default)]
    pub downloaded_bytes: u64,
    #[serde(default)]
    pub total_bytes: Option<u64>,
    pub started_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub recovered_after_restart: bool,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTransactionJournal {
    pub schema_version: u32,
    pub active: BTreeMap<String, PluginTransactionRecord>,
    pub history: Vec<PluginTransactionRecord>,
}

impl Default for PluginTransactionJournal {
    fn default() -> Self {
        Self {
            schema_version: JOURNAL_SCHEMA_VERSION,
            active: BTreeMap::new(),
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalPluginStatusSnapshot {
    pub registry: LocalPluginRegistry,
    pub transactions: PluginTransactionJournal,
    #[serde(default)]
    pub runtime: PluginRuntimeTelemetrySnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRecoveryReport {
    pub completed_transactions: usize,
    pub rolled_back_transactions: usize,
    pub cleaned_paths: usize,
    #[serde(default)]
    pub errors: Vec<String>,
}

pub(super) fn journal_path(plugin_root: &Path) -> PathBuf {
    plugin_root.join("transactions.json")
}

pub(super) fn load_journal(plugin_root: &Path) -> Result<PluginTransactionJournal> {
    let path = journal_path(plugin_root);
    if !path.exists() {
        return Ok(PluginTransactionJournal::default());
    }
    let metadata = fs::metadata(&path).with_context(|| {
        format!(
            "read Plugin transaction journal metadata: {}",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() > MAX_JOURNAL_BYTES {
        bail!("Plugin transaction journal is invalid or exceeds the size limit");
    }
    let journal: PluginTransactionJournal = serde_json::from_slice(
        fs::read(&path)
            .with_context(|| format!("read Plugin transaction journal: {}", path.display()))?
            .as_slice(),
    )
    .context("parse Plugin transaction journal")?;
    validate_journal(&journal)?;
    Ok(journal)
}

pub(super) fn begin_transaction(plugin_root: &Path, record: PluginTransactionRecord) -> Result<()> {
    let mut journal = load_journal(plugin_root)?;
    if journal.active.contains_key(record.transaction_id.as_str())
        || journal
            .active
            .values()
            .any(|active| active.plugin_id == record.plugin_id)
    {
        bail!("another Plugin transaction is already active for this Plugin");
    }
    journal.active.insert(record.transaction_id.clone(), record);
    save_journal(plugin_root, &journal)
}

pub(super) fn transition_transaction(
    plugin_root: &Path,
    transaction_id: &str,
    status: PluginInstallStatus,
    updated_at: String,
) -> Result<()> {
    let mut journal = load_journal(plugin_root)?;
    let record = journal
        .active
        .get_mut(transaction_id)
        .context("Plugin transaction is not active")?;
    record.status = status;
    record.updated_at = updated_at;
    save_journal(plugin_root, &journal)
}

pub(super) fn update_download_progress(
    plugin_root: &Path,
    transaction_id: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    updated_at: String,
) -> Result<()> {
    let mut journal = load_journal(plugin_root)?;
    let record = journal
        .active
        .get_mut(transaction_id)
        .context("Plugin download transaction is not active")?;
    if record.status != PluginInstallStatus::Downloading {
        bail!("Plugin transaction is not downloading");
    }
    if downloaded_bytes < record.downloaded_bytes {
        bail!("Plugin download progress cannot move backwards");
    }
    if let (Some(previous), Some(next)) = (record.total_bytes, total_bytes) {
        if previous != next {
            bail!("Plugin download total bytes cannot change");
        }
    }
    let total_bytes = total_bytes.or(record.total_bytes);
    if total_bytes.is_some_and(|total| downloaded_bytes > total) {
        bail!("Plugin downloaded bytes exceed the declared total");
    }
    if record.downloaded_bytes == downloaded_bytes && record.total_bytes == total_bytes {
        return Ok(());
    }
    record.downloaded_bytes = downloaded_bytes;
    record.total_bytes = total_bytes;
    record.updated_at = updated_at;
    save_journal(plugin_root, &journal)
}

pub(super) fn finish_transaction(
    plugin_root: &Path,
    transaction_id: &str,
    status: PluginInstallStatus,
    completed_at: String,
    recovered_after_restart: bool,
    last_error: Option<String>,
) -> Result<()> {
    let mut journal = load_journal(plugin_root)?;
    let mut record = journal
        .active
        .remove(transaction_id)
        .context("Plugin transaction is not active")?;
    record.status = status;
    record.updated_at = completed_at.clone();
    record.completed_at = Some(completed_at);
    record.recovered_after_restart = recovered_after_restart;
    record.last_error = last_error;
    journal.history.push(record);
    if journal.history.len() > MAX_HISTORY_ENTRIES {
        let remove = journal.history.len() - MAX_HISTORY_ENTRIES;
        journal.history.drain(0..remove);
    }
    save_journal(plugin_root, &journal)
}

fn save_journal(plugin_root: &Path, journal: &PluginTransactionJournal) -> Result<()> {
    fs::create_dir_all(plugin_root)
        .with_context(|| format!("create Plugin storage root: {}", plugin_root.display()))?;
    let payload =
        serde_json::to_vec_pretty(journal).context("serialize Plugin transaction journal")?;
    if payload.len() as u64 > MAX_JOURNAL_BYTES {
        bail!("Plugin transaction journal exceeds the size limit");
    }
    let mut temporary = NamedTempFile::new_in(plugin_root)
        .context("create temporary Plugin transaction journal")?;
    temporary
        .write_all(payload.as_slice())
        .context("write temporary Plugin transaction journal")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync temporary Plugin transaction journal")?;
    temporary
        .persist(journal_path(plugin_root))
        .map_err(|error| error.error)
        .context("atomically replace Plugin transaction journal")?;
    Ok(())
}

fn validate_journal(journal: &PluginTransactionJournal) -> Result<()> {
    if journal.schema_version != JOURNAL_SCHEMA_VERSION {
        bail!("unsupported Plugin transaction journal schema version");
    }
    if journal.history.len() > MAX_HISTORY_ENTRIES {
        bail!("Plugin transaction journal history exceeds the entry limit");
    }
    for (transaction_id, record) in &journal.active {
        if transaction_id != &record.transaction_id || record.completed_at.is_some() {
            bail!("Plugin transaction journal contains an invalid active record");
        }
        validate_transaction_paths(record)?;
    }
    for record in &journal.history {
        if record.completed_at.is_none() {
            bail!("Plugin transaction history contains an incomplete record");
        }
        validate_transaction_paths(record)?;
    }
    Ok(())
}

fn validate_transaction_paths(record: &PluginTransactionRecord) -> Result<()> {
    if record
        .total_bytes
        .is_some_and(|total| record.downloaded_bytes > total)
    {
        bail!("Plugin transaction journal contains invalid download progress");
    }
    for path in [
        record.relative_staging_path.as_deref(),
        record.relative_final_path.as_deref(),
        record.relative_storage_path.as_deref(),
        record.relative_trash_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if path.starts_with('/')
            || path.contains('\\')
            || path.contains(':')
            || path
                .split('/')
                .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        {
            bail!("Plugin transaction journal contains an unsafe relative path");
        }
    }
    Ok(())
}
