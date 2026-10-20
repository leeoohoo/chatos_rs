use super::models::RemoteCredentialBundle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use uuid::Uuid;

const CURRENT_VERSION: u32 = 1;
const MAXIMUM_INDEX_BYTES: u64 = 4 * 1_024 * 1_024;
const MAXIMUM_OWNERS: usize = 32;
const MAXIMUM_CONNECTIONS_PER_OWNER: usize = 10_000;
// Windows Credential Manager limits a generic credential blob to 2,560 bytes.
// Keep room for the keyring backend's representation and JSON field names.
const MAXIMUM_BUNDLE_BYTES: usize = 2_048;
const SECRET_SERVICE: &str = "com.chatos.windows.remote-connections";

pub(super) trait RemoteCredentialVault: Send + Sync {
    fn load(
        &self,
        owner_user_id: &str,
        connection_id: &str,
    ) -> Result<Option<RemoteCredentialBundle>, String>;
    fn save(
        &self,
        owner_user_id: &str,
        connection_id: &str,
        credentials: &RemoteCredentialBundle,
    ) -> Result<(), String>;
    fn delete(&self, owner_user_id: &str, connection_id: &str) -> Result<(), String>;
    fn reconcile(
        &self,
        owner_user_id: &str,
        active_connection_ids: &BTreeSet<String>,
    ) -> Result<(), String>;
}

trait RemoteSecretStore: Send + Sync {
    fn load(&self, account: &str) -> Result<Option<String>, String>;
    fn save(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

struct PlatformRemoteSecretStore;

pub(super) struct PlatformRemoteCredentialVault {
    path: PathBuf,
    secrets: Arc<dyn RemoteSecretStore>,
    lock: Mutex<()>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialIndex {
    version: u32,
    #[serde(default)]
    owners: BTreeMap<String, BTreeSet<String>>,
}

impl PlatformRemoteCredentialVault {
    pub(super) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            path: app_data_directory
                .join("remote-connections")
                .join("credential-index.json"),
            secrets: Arc::new(PlatformRemoteSecretStore),
            lock: Mutex::new(()),
        }
    }

    #[cfg(test)]
    fn with_secrets(path: PathBuf, secrets: Arc<dyn RemoteSecretStore>) -> Self {
        Self {
            path,
            secrets,
            lock: Mutex::new(()),
        }
    }

    fn guard(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.lock
            .lock()
            .map_err(|_| "远端连接凭据暂时不可用。".to_owned())
    }

    fn load_index(&self) -> Result<CredentialIndex, String> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CredentialIndex {
                    version: CURRENT_VERSION,
                    owners: BTreeMap::new(),
                })
            }
            Err(_) => return Err(index_error()),
        };
        let metadata = file.metadata().map_err(|_| index_error())?;
        if !metadata.is_file() || metadata.len() > MAXIMUM_INDEX_BYTES {
            return Err(index_error());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAXIMUM_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| index_error())?;
        if bytes.len() as u64 > MAXIMUM_INDEX_BYTES {
            return Err(index_error());
        }
        let index: CredentialIndex = serde_json::from_slice(&bytes).map_err(|_| index_error())?;
        validate_index(&index)?;
        Ok(index)
    }

    fn save_index(&self, index: &CredentialIndex) -> Result<(), String> {
        validate_index(index)?;
        let bytes = serde_json::to_vec(index).map_err(|_| index_error())?;
        if bytes.len() as u64 > MAXIMUM_INDEX_BYTES {
            return Err(index_error());
        }
        let parent = self.path.parent().ok_or_else(index_error)?;
        fs::create_dir_all(parent).map_err(|_| index_error())?;
        reject_symbolic_link(&self.path)?;
        let temporary = parent.join(format!(".credential-index-{}.tmp", Uuid::new_v4()));
        let result =
            write_private_file(&temporary, &bytes).and_then(|_| fs::rename(&temporary, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(index_error());
        }
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    }
}

impl RemoteCredentialVault for PlatformRemoteCredentialVault {
    fn load(
        &self,
        owner_user_id: &str,
        connection_id: &str,
    ) -> Result<Option<RemoteCredentialBundle>, String> {
        validate_identifier(owner_user_id)?;
        validate_identifier(connection_id)?;
        let _guard = self.guard()?;
        let Some(encoded) = self
            .secrets
            .load(&secret_account(owner_user_id, connection_id))?
        else {
            return Ok(None);
        };
        if encoded.len() > MAXIMUM_BUNDLE_BYTES {
            return Err(secret_error());
        }
        let credentials = serde_json::from_str(&encoded).map_err(|_| secret_error())?;
        Ok(Some(credentials))
    }

    fn save(
        &self,
        owner_user_id: &str,
        connection_id: &str,
        credentials: &RemoteCredentialBundle,
    ) -> Result<(), String> {
        validate_identifier(owner_user_id)?;
        validate_identifier(connection_id)?;
        if credentials.is_empty() {
            return self.delete(owner_user_id, connection_id);
        }
        let encoded = serde_json::to_string(credentials).map_err(|_| secret_error())?;
        if encoded.len() > MAXIMUM_BUNDLE_BYTES {
            return Err(secret_error());
        }
        let _guard = self.guard()?;
        self.secrets
            .save(&secret_account(owner_user_id, connection_id), &encoded)?;
        let mut index = self.load_index()?;
        index
            .owners
            .entry(owner_user_id.to_owned())
            .or_default()
            .insert(connection_id.to_owned());
        self.save_index(&index)
    }

    fn delete(&self, owner_user_id: &str, connection_id: &str) -> Result<(), String> {
        validate_identifier(owner_user_id)?;
        validate_identifier(connection_id)?;
        let _guard = self.guard()?;
        self.secrets
            .delete(&secret_account(owner_user_id, connection_id))?;
        let mut index = self.load_index()?;
        if let Some(connections) = index.owners.get_mut(owner_user_id) {
            connections.remove(connection_id);
            if connections.is_empty() {
                index.owners.remove(owner_user_id);
            }
        }
        self.save_index(&index)
    }

    fn reconcile(
        &self,
        owner_user_id: &str,
        active_connection_ids: &BTreeSet<String>,
    ) -> Result<(), String> {
        validate_identifier(owner_user_id)?;
        for connection_id in active_connection_ids {
            validate_identifier(connection_id)?;
        }
        let _guard = self.guard()?;
        let mut index = self.load_index()?;
        let tracked = index.owners.get(owner_user_id).cloned().unwrap_or_default();
        let mut remaining = tracked.clone();
        for connection_id in tracked.difference(active_connection_ids) {
            self.secrets
                .delete(&secret_account(owner_user_id, connection_id))?;
            remaining.remove(connection_id);
        }
        if remaining.is_empty() {
            index.owners.remove(owner_user_id);
        } else {
            index.owners.insert(owner_user_id.to_owned(), remaining);
        }
        self.save_index(&index)
    }
}

fn validate_index(index: &CredentialIndex) -> Result<(), String> {
    if index.version != CURRENT_VERSION || index.owners.len() > MAXIMUM_OWNERS {
        return Err(index_error());
    }
    for (owner, connections) in &index.owners {
        validate_identifier(owner)?;
        if connections.len() > MAXIMUM_CONNECTIONS_PER_OWNER {
            return Err(index_error());
        }
        for connection in connections {
            validate_identifier(connection)?;
        }
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        Err(index_error())
    } else {
        Ok(())
    }
}

fn secret_account(owner_user_id: &str, connection_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(owner_user_id.as_bytes());
    digest.update([0]);
    digest.update(connection_id.as_bytes());
    format!("remote-{:x}", digest.finalize())
}

fn reject_symbolic_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(index_error()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(index_error()),
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn index_error() -> String {
    "无法读取或保存远端连接凭据索引。".into()
}

fn secret_error() -> String {
    "无法读取系统凭据存储中的远端连接凭据。".into()
}

fn safe_secret_error(error: impl Display) -> String {
    let _ = error;
    secret_error()
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl RemoteSecretStore for PlatformRemoteSecretStore {
    fn load(&self, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SECRET_SERVICE, account).map_err(safe_secret_error)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(safe_secret_error(error)),
        }
    }

    fn save(&self, account: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(SECRET_SERVICE, account)
            .map_err(safe_secret_error)?
            .set_password(value)
            .map_err(safe_secret_error)
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SECRET_SERVICE, account).map_err(safe_secret_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(safe_secret_error(error)),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl RemoteSecretStore for PlatformRemoteSecretStore {
    fn load(&self, _account: &str) -> Result<Option<String>, String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn save(&self, _account: &str, _value: &str) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn delete(&self, _account: &str) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }
}

#[cfg(test)]
pub(super) mod tests_support {
    use super::*;

    #[derive(Default)]
    pub(super) struct MemoryRemoteSecretStore {
        values: Mutex<BTreeMap<String, String>>,
    }

    impl RemoteSecretStore for MemoryRemoteSecretStore {
        fn load(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(account).cloned())
        }

        fn save(&self, account: &str, value: &str) -> Result<(), String> {
            self.values
                .lock()
                .unwrap()
                .insert(account.to_owned(), value.to_owned());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(account);
            Ok(())
        }
    }

    pub(in crate::remote_connection) fn memory_vault(
        path: PathBuf,
    ) -> PlatformRemoteCredentialVault {
        PlatformRemoteCredentialVault::with_secrets(
            path,
            Arc::new(MemoryRemoteSecretStore::default()),
        )
    }
}
