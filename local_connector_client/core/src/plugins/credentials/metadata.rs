// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::{PluginCredentialMetadata, PluginCredentialScope};

const CREDENTIAL_INDEX_SCHEMA_VERSION: u32 = 1;
const MAX_CREDENTIAL_INDEX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredCredentialRecord {
    pub(super) scope: PluginCredentialScope,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

impl StoredCredentialRecord {
    pub(super) fn public_metadata(&self) -> PluginCredentialMetadata {
        PluginCredentialMetadata {
            plugin_id: self.scope.plugin_id.clone(),
            release_id: self.scope.release_id.clone(),
            component_key: self.scope.component_key.clone(),
            secret_name: self.scope.secret_name.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CredentialIndex {
    schema_version: u32,
    pub(super) credentials: BTreeMap<String, StoredCredentialRecord>,
}

impl Default for CredentialIndex {
    fn default() -> Self {
        Self {
            schema_version: CREDENTIAL_INDEX_SCHEMA_VERSION,
            credentials: BTreeMap::new(),
        }
    }
}

pub(super) fn metadata_path(plugin_root: &Path) -> PathBuf {
    plugin_root.join("credentials.json")
}

pub(super) fn load_index(plugin_root: &Path) -> Result<CredentialIndex> {
    let path = metadata_path(plugin_root);
    if !path.exists() {
        return Ok(CredentialIndex::default());
    }
    let metadata = fs::metadata(&path)
        .with_context(|| format!("read Plugin credential metadata {}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_CREDENTIAL_INDEX_BYTES {
        bail!("Plugin credential metadata is invalid or exceeds the size limit");
    }
    let index: CredentialIndex = serde_json::from_slice(
        fs::read(&path)
            .with_context(|| format!("read Plugin credential metadata {}", path.display()))?
            .as_slice(),
    )
    .context("parse Plugin credential metadata")?;
    if index.schema_version != CREDENTIAL_INDEX_SCHEMA_VERSION {
        bail!("unsupported Plugin credential metadata schema version");
    }
    for (scope_hash, record) in &index.credentials {
        record.scope.validate()?;
        if scope_hash != &record.scope.scope_hash() {
            bail!("Plugin credential metadata scope hash does not match its scope");
        }
    }
    Ok(index)
}

pub(super) fn save_index(plugin_root: &Path, index: &CredentialIndex) -> Result<()> {
    fs::create_dir_all(plugin_root).with_context(|| {
        format!(
            "create Plugin credential metadata directory {}",
            plugin_root.display()
        )
    })?;
    let payload =
        serde_json::to_vec_pretty(index).context("serialize Plugin credential metadata")?;
    if payload.len() as u64 > MAX_CREDENTIAL_INDEX_BYTES {
        bail!("Plugin credential metadata exceeds the size limit");
    }
    let mut temporary = NamedTempFile::new_in(plugin_root)
        .context("create temporary Plugin credential metadata")?;
    temporary
        .write_all(payload.as_slice())
        .context("write temporary Plugin credential metadata")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync temporary Plugin credential metadata")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .context("restrict temporary Plugin credential metadata")?;
    }
    temporary
        .persist(metadata_path(plugin_root))
        .map_err(|error| error.error)
        .context("atomically replace Plugin credential metadata")?;
    sync_directory(plugin_root)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("open Plugin credential directory {}", path.display()))?
        .sync_all()
        .with_context(|| format!("sync Plugin credential directory {}", path.display()))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}
