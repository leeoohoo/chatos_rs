// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::secure_storage::SecureStorage;

use self::handles::PluginSecretHandleRegistry;
use self::metadata::{load_index, save_index, StoredCredentialRecord};

mod handles;
mod metadata;

const PLUGIN_CREDENTIAL_SERVICE: &str = "Chat OS Local Connector Plugin Credential";
const MAX_SECRET_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCredentialScope {
    pub owner_user_id: String,
    pub device_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub secret_name: String,
}

impl PluginCredentialScope {
    pub fn new(
        owner_user_id: impl Into<String>,
        device_id: impl Into<String>,
        plugin_id: impl Into<String>,
        release_id: impl Into<String>,
        component_key: impl Into<String>,
        secret_name: impl Into<String>,
    ) -> Result<Self> {
        let scope = Self {
            owner_user_id: owner_user_id.into(),
            device_id: device_id.into(),
            plugin_id: plugin_id.into(),
            release_id: release_id.into(),
            component_key: component_key.into(),
            secret_name: secret_name.into(),
        };
        scope.validate()?;
        Ok(scope)
    }

    fn validate(&self) -> Result<()> {
        validate_scope_value("owner user id", self.owner_user_id.as_str(), 256, false)?;
        validate_scope_value("device id", self.device_id.as_str(), 256, false)?;
        validate_scope_value("Plugin id", self.plugin_id.as_str(), 256, false)?;
        validate_scope_value("Plugin Release id", self.release_id.as_str(), 256, false)?;
        validate_scope_value(
            "Plugin component key",
            self.component_key.as_str(),
            128,
            true,
        )?;
        validate_scope_value("Plugin secret name", self.secret_name.as_str(), 128, true)
    }

    fn scope_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"chatos-plugin-credential-scope-v1\0");
        for value in [
            self.owner_user_id.as_str(),
            self.device_id.as_str(),
            self.plugin_id.as_str(),
            self.release_id.as_str(),
            self.component_key.as_str(),
            self.secret_name.as_str(),
        ] {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCredentialMetadata {
    pub plugin_id: String,
    pub release_id: String,
    pub component_key: String,
    pub secret_name: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct ResolvedPluginSecret(Vec<u8>);

impl ResolvedPluginSecret {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl fmt::Debug for ResolvedPluginSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResolvedPluginSecret([REDACTED])")
    }
}

impl Drop for ResolvedPluginSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Debug, Clone)]
pub struct PluginCredentialVault {
    plugin_root: PathBuf,
    secret_root: PathBuf,
    storage: SecureStorage,
    operation_lock: Arc<Mutex<()>>,
    handles: PluginSecretHandleRegistry,
}

impl PluginCredentialVault {
    pub(crate) fn for_state_path(state_path: &Path) -> Self {
        let app_data = state_path.parent().unwrap_or_else(|| Path::new("."));
        Self::new(
            app_data.join("plugins"),
            app_data.join("secure-storage").join("plugin-credentials"),
            SecureStorage::platform(PLUGIN_CREDENTIAL_SERVICE),
        )
    }

    fn new(plugin_root: PathBuf, secret_root: PathBuf, storage: SecureStorage) -> Self {
        Self {
            plugin_root,
            secret_root,
            storage,
            operation_lock: Arc::new(Mutex::new(())),
            handles: PluginSecretHandleRegistry::default(),
        }
    }

    #[cfg(test)]
    pub(super) fn in_memory(app_data: &Path) -> Self {
        Self::new(
            app_data.join("plugins"),
            app_data.join("secure-storage").join("plugin-credentials"),
            SecureStorage::in_memory(PLUGIN_CREDENTIAL_SERVICE),
        )
    }

    pub fn list(
        &self,
        owner_user_id: &str,
        device_id: &str,
        plugin_id: &str,
        release_id: &str,
    ) -> Result<Vec<PluginCredentialMetadata>> {
        validate_scope_value("owner user id", owner_user_id, 256, false)?;
        validate_scope_value("device id", device_id, 256, false)?;
        validate_scope_value("Plugin id", plugin_id, 256, false)?;
        validate_scope_value("Plugin Release id", release_id, 256, false)?;
        let _guard = self.operation_guard()?;
        let mut credentials = load_index(self.plugin_root.as_path())?
            .credentials
            .into_values()
            .filter(|record| {
                record.scope.owner_user_id == owner_user_id
                    && record.scope.device_id == device_id
                    && record.scope.plugin_id == plugin_id
                    && record.scope.release_id == release_id
            })
            .map(|record| record.public_metadata())
            .collect::<Vec<_>>();
        credentials.sort_by(|left, right| {
            left.component_key
                .cmp(&right.component_key)
                .then_with(|| left.secret_name.cmp(&right.secret_name))
        });
        Ok(credentials)
    }

    pub fn upsert(
        &self,
        scope: &PluginCredentialScope,
        value: &[u8],
    ) -> Result<PluginCredentialMetadata> {
        scope.validate()?;
        if value.is_empty() || value.len() > MAX_SECRET_BYTES {
            bail!("Plugin credential must contain between 1 byte and 64 KiB");
        }
        let _guard = self.operation_guard()?;
        let scope_hash = scope.scope_hash();
        let path = self.secret_path(scope_hash.as_str());
        let account = storage_account(scope_hash.as_str());
        let mut index = load_index(self.plugin_root.as_path())?;
        let previous_secret = self.storage.load(account.as_str(), path.as_path())?;
        let previous_record = index.credentials.get(scope_hash.as_str()).cloned();
        self.storage
            .save(account.as_str(), path.as_path(), value)
            .context("store Plugin credential")?;

        let now = Utc::now().to_rfc3339();
        let record = StoredCredentialRecord {
            scope: scope.clone(),
            created_at: previous_record
                .as_ref()
                .map(|record| record.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        index.credentials.insert(scope_hash.clone(), record.clone());
        if let Err(error) = save_index(self.plugin_root.as_path(), &index) {
            match previous_secret {
                Some(mut previous) => {
                    let _ = self
                        .storage
                        .save(account.as_str(), path.as_path(), &previous);
                    previous.fill(0);
                }
                None => {
                    let _ = self.storage.delete(account.as_str(), path.as_path());
                }
            }
            return Err(error);
        }
        self.handles.revoke_scope(scope_hash.as_str())?;
        Ok(record.public_metadata())
    }

    pub fn delete(&self, scope: &PluginCredentialScope) -> Result<bool> {
        scope.validate()?;
        let _guard = self.operation_guard()?;
        let scope_hash = scope.scope_hash();
        let mut index = load_index(self.plugin_root.as_path())?;
        if !index.credentials.contains_key(scope_hash.as_str()) {
            return Ok(false);
        }
        self.handles.revoke_scope(scope_hash.as_str())?;
        let path = self.secret_path(scope_hash.as_str());
        self.storage
            .delete(
                storage_account(scope_hash.as_str()).as_str(),
                path.as_path(),
            )
            .context("delete Plugin credential")?;
        index.credentials.remove(scope_hash.as_str());
        save_index(self.plugin_root.as_path(), &index)?;
        Ok(true)
    }

    pub fn issue_handle(&self, scope: &PluginCredentialScope, ttl: Duration) -> Result<String> {
        scope.validate()?;
        let _guard = self.operation_guard()?;
        let scope_hash = scope.scope_hash();
        let index = load_index(self.plugin_root.as_path())?;
        let record = index
            .credentials
            .get(scope_hash.as_str())
            .context("Plugin credential does not exist")?;
        if record.scope != *scope {
            bail!("Plugin credential metadata scope mismatch");
        }
        let path = self.secret_path(scope_hash.as_str());
        let mut secret = self
            .storage
            .load(
                storage_account(scope_hash.as_str()).as_str(),
                path.as_path(),
            )?
            .context("Plugin credential secure value is unavailable")?;
        secret.fill(0);
        self.handles.issue(scope_hash.as_str(), ttl)
    }

    pub fn resolve_handle(
        &self,
        handle: &str,
        scope: &PluginCredentialScope,
    ) -> Result<ResolvedPluginSecret> {
        scope.validate()?;
        let _guard = self.operation_guard()?;
        let scope_hash = scope.scope_hash();
        self.handles.validate(handle, scope_hash.as_str())?;
        let index = load_index(self.plugin_root.as_path())?;
        let record = index
            .credentials
            .get(scope_hash.as_str())
            .context("Plugin credential no longer exists")?;
        if record.scope != *scope {
            bail!("Plugin credential metadata scope mismatch");
        }
        let path = self.secret_path(scope_hash.as_str());
        let secret = self
            .storage
            .load(
                storage_account(scope_hash.as_str()).as_str(),
                path.as_path(),
            )?
            .context("Plugin credential secure value is unavailable")?;
        Ok(ResolvedPluginSecret(secret))
    }

    pub fn revoke_handle(&self, handle: &str) -> Result<bool> {
        self.handles.revoke(handle)
    }

    pub fn purge_plugin(&self, plugin_id: &str) -> Result<usize> {
        self.purge_matching(|record| record.scope.plugin_id == plugin_id)
    }

    pub fn purge_release(&self, plugin_id: &str, release_id: &str) -> Result<usize> {
        self.purge_matching(|record| {
            record.scope.plugin_id == plugin_id && record.scope.release_id == release_id
        })
    }

    fn purge_matching(&self, predicate: impl Fn(&StoredCredentialRecord) -> bool) -> Result<usize> {
        let _guard = self.operation_guard()?;
        let mut index = load_index(self.plugin_root.as_path())?;
        let scope_hashes = index
            .credentials
            .iter()
            .filter_map(|(scope_hash, record)| predicate(record).then_some(scope_hash.clone()))
            .collect::<Vec<_>>();
        for scope_hash in &scope_hashes {
            self.handles.revoke_scope(scope_hash.as_str())?;
            let path = self.secret_path(scope_hash.as_str());
            self.storage
                .delete(
                    storage_account(scope_hash.as_str()).as_str(),
                    path.as_path(),
                )
                .context("purge Plugin credential")?;
            index.credentials.remove(scope_hash.as_str());
        }
        if !scope_hashes.is_empty() {
            save_index(self.plugin_root.as_path(), &index)?;
        }
        Ok(scope_hashes.len())
    }

    fn operation_guard(&self) -> Result<MutexGuard<'_, ()>> {
        self.operation_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin credential operation lock is poisoned"))
    }

    fn secret_path(&self, scope_hash: &str) -> PathBuf {
        self.secret_root.join(format!("{scope_hash}.bin"))
    }
}

fn storage_account(scope_hash: &str) -> String {
    format!("chatos-plugin-credential-{scope_hash}")
}

fn validate_scope_value(
    label: &str,
    value: &str,
    max_bytes: usize,
    path_segment: bool,
) -> Result<()> {
    if value.is_empty() || value.len() > max_bytes || value.trim() != value {
        bail!("{label} is missing or invalid");
    }
    if value.chars().any(char::is_control) {
        bail!("{label} contains control characters");
    }
    if path_segment
        && !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("{label} contains unsupported characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
