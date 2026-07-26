// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
#[cfg(any(windows, all(not(windows), not(target_os = "macos"))))]
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[cfg(any(windows, all(not(windows), not(target_os = "macos"))))]
use anyhow::Context;
use anyhow::Result;

#[cfg(target_os = "macos")]
mod macos_keychain;
#[cfg(windows)]
mod windows_dpapi;

trait SecureStorageBackend: Send + Sync {
    fn load(&self, service: &str, account: &str, path: &Path) -> Result<Option<Vec<u8>>>;
    fn save(&self, service: &str, account: &str, path: &Path, value: &[u8]) -> Result<()>;
    fn delete(&self, service: &str, account: &str, path: &Path) -> Result<bool>;
}

#[derive(Clone)]
pub(crate) struct SecureStorage {
    service: Arc<str>,
    backend: Arc<dyn SecureStorageBackend>,
}

impl fmt::Debug for SecureStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecureStorage")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

impl SecureStorage {
    pub(crate) fn platform(service: impl Into<Arc<str>>) -> Self {
        Self {
            service: service.into(),
            backend: Arc::new(PlatformSecureStorage),
        }
    }

    pub(crate) fn load(&self, account: &str, path: &Path) -> Result<Option<Vec<u8>>> {
        self.backend.load(self.service.as_ref(), account, path)
    }

    pub(crate) fn save(&self, account: &str, path: &Path, value: &[u8]) -> Result<()> {
        self.backend
            .save(self.service.as_ref(), account, path, value)
    }

    pub(crate) fn delete(&self, account: &str, path: &Path) -> Result<bool> {
        self.backend.delete(self.service.as_ref(), account, path)
    }

    #[cfg(test)]
    pub(crate) fn in_memory(service: impl Into<Arc<str>>) -> Self {
        Self {
            service: service.into(),
            backend: Arc::new(MemorySecureStorage::default()),
        }
    }
}

#[derive(Debug)]
struct PlatformSecureStorage;

impl SecureStorageBackend for PlatformSecureStorage {
    fn load(&self, service: &str, account: &str, path: &Path) -> Result<Option<Vec<u8>>> {
        #[cfg(target_os = "macos")]
        {
            let _ = path;
            macos_keychain::load(service, account)
        }
        #[cfg(windows)]
        {
            let _ = (service, account);
            windows_dpapi::load(path)
        }
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            let _ = (service, account);
            load_restricted_file(path)
        }
    }

    fn save(&self, service: &str, account: &str, path: &Path, value: &[u8]) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let _ = path;
            macos_keychain::save(service, account, value)
        }
        #[cfg(windows)]
        {
            let _ = (service, account);
            windows_dpapi::save(path, value)
        }
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            let _ = (service, account);
            save_restricted_file(path, value)
        }
    }

    fn delete(&self, service: &str, account: &str, path: &Path) -> Result<bool> {
        #[cfg(target_os = "macos")]
        {
            let _ = path;
            macos_keychain::delete(service, account)
        }
        #[cfg(windows)]
        {
            let _ = (service, account);
            delete_file(path)
        }
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            let _ = (service, account);
            delete_file(path)
        }
    }
}

#[cfg(any(windows, all(not(windows), not(target_os = "macos"))))]
fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create secure storage directory {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn load_restricted_file(path: &Path) -> Result<Option<Vec<u8>>> {
    if !path.is_file() {
        return Ok(None);
    }
    fs::read(path)
        .map(Some)
        .with_context(|| format!("read secure storage file {}", path.display()))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn save_restricted_file(path: &Path, value: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure_parent(path)?;
    fs::write(path, value)
        .with_context(|| format!("write secure storage file {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restrict secure storage file {}", path.display()))
}

#[cfg(any(windows, all(not(windows), not(target_os = "macos"))))]
fn delete_file(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(path)
        .with_context(|| format!("delete secure storage file {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
#[derive(Debug, Default)]
struct MemorySecureStorage {
    values: std::sync::Mutex<std::collections::BTreeMap<(String, String), Vec<u8>>>,
}

#[cfg(test)]
impl SecureStorageBackend for MemorySecureStorage {
    fn load(&self, service: &str, account: &str, _path: &Path) -> Result<Option<Vec<u8>>> {
        Ok(self
            .values
            .lock()
            .map_err(|_| anyhow::anyhow!("in-memory secure storage lock is poisoned"))?
            .get(&(service.to_string(), account.to_string()))
            .cloned())
    }

    fn save(&self, service: &str, account: &str, _path: &Path, value: &[u8]) -> Result<()> {
        self.values
            .lock()
            .map_err(|_| anyhow::anyhow!("in-memory secure storage lock is poisoned"))?
            .insert((service.to_string(), account.to_string()), value.to_vec());
        Ok(())
    }

    fn delete(&self, service: &str, account: &str, _path: &Path) -> Result<bool> {
        Ok(self
            .values
            .lock()
            .map_err(|_| anyhow::anyhow!("in-memory secure storage lock is poisoned"))?
            .remove(&(service.to_string(), account.to_string()))
            .is_some())
    }
}
