// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;

#[cfg(windows)]
use std::path::{Path, PathBuf};

#[cfg(windows)]
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const LOCAL_AGENT_CREDENTIAL_SERVICE: &str = "com.chatos.local-agent.credentials.v1";
pub const WINDOWS_LOCAL_AGENT_CREDENTIAL_RESOURCE: &str =
    "ChatOS.Windows.LocalAgent.Credentials.v1";
#[cfg(windows)]
const WINDOWS_PROTECTED_KEY_RELATIVE_DIRECTORY: [&str; 4] =
    ["ChatOS", "LocalAgent", "ProtectedKeys", ""];
#[cfg(windows)]
const WINDOWS_DPAPI_FILE_SUFFIX: &str = ".dpapi";
#[cfg(windows)]
const WINDOWS_DPAPI_ENTROPY_PREFIX: &[u8] = b"chatos-local-agent-dpapi-v1\n";
#[cfg(windows)]
const MAXIMUM_WINDOWS_PROTECTED_KEY_BYTES: u64 = 64 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalAgentPlatformCredentialError {
    #[error("local Agent credential identity is invalid")]
    InvalidIdentity,
    #[error("local Agent credential is unavailable")]
    Unavailable,
}

pub trait LocalAgentPlatformCredentialReader: Send + Sync {
    fn read(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError>;
}

/// Reads device-bound binary keys which cannot be represented safely by the
/// Windows PasswordVault string contract. The production implementation uses
/// DPAPI CurrentUser and returns zeroizing memory.
pub trait LocalAgentPlatformDeviceKeyReader: Send + Sync {
    fn read_device_key(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError>;
}

pub fn platform_credential_account_key(
    owner_user_id: &str,
    reference: &str,
) -> Result<String, LocalAgentPlatformCredentialError> {
    validate_identity(owner_user_id)?;
    validate_identity(reference)?;
    Ok(format!(
        "v1:{}:{owner_user_id}{reference}",
        owner_user_id.len()
    ))
}

fn validate_identity(value: &str) -> Result<(), LocalAgentPlatformCredentialError> {
    if value.is_empty()
        || value.len() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(LocalAgentPlatformCredentialError::InvalidIdentity)
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub struct MacOsLocalAgentCredentialReader {
    service: String,
}

#[cfg(target_os = "macos")]
impl MacOsLocalAgentCredentialReader {
    pub fn production() -> Self {
        Self {
            service: LOCAL_AGENT_CREDENTIAL_SERVICE.to_string(),
        }
    }

    #[cfg(test)]
    fn for_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

#[cfg(target_os = "macos")]
impl fmt::Debug for MacOsLocalAgentCredentialReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacOsLocalAgentCredentialReader")
            .field("service", &"[KEYCHAIN SERVICE]")
            .finish()
    }
}

#[cfg(target_os = "macos")]
impl Default for MacOsLocalAgentCredentialReader {
    fn default() -> Self {
        Self::production()
    }
}

#[cfg(target_os = "macos")]
impl LocalAgentPlatformCredentialReader for MacOsLocalAgentCredentialReader {
    fn read(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        let account = platform_credential_account_key(owner_user_id, reference)?;
        let secret = security_framework::passwords::get_generic_password(&self.service, &account)
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        if secret.is_empty() {
            Err(LocalAgentPlatformCredentialError::Unavailable)
        } else {
            Ok(Zeroizing::new(secret))
        }
    }
}

#[cfg(target_os = "macos")]
impl LocalAgentPlatformDeviceKeyReader for MacOsLocalAgentCredentialReader {
    fn read_device_key(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        self.read(owner_user_id, reference)
    }
}

#[cfg(windows)]
pub struct WindowsLocalAgentCredentialReader {
    resource: String,
    mta_cookie: usize,
}

#[cfg(windows)]
impl WindowsLocalAgentCredentialReader {
    pub fn production() -> Result<Self, LocalAgentPlatformCredentialError> {
        use windows::Win32::System::Com::CoIncrementMTAUsage;

        // SAFETY: CoIncrementMTAUsage has no pointer inputs and returns an
        // opaque process-wide cookie released by this reader's Drop.
        let cookie = unsafe { CoIncrementMTAUsage() }
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        Ok(Self {
            resource: WINDOWS_LOCAL_AGENT_CREDENTIAL_RESOURCE.to_string(),
            mta_cookie: cookie.0 as usize,
        })
    }
}

#[cfg(windows)]
impl fmt::Debug for WindowsLocalAgentCredentialReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowsLocalAgentCredentialReader")
            .field("resource", &"[CREDENTIAL MANAGER RESOURCE]")
            .finish()
    }
}

#[cfg(windows)]
impl Drop for WindowsLocalAgentCredentialReader {
    fn drop(&mut self) {
        use windows::Win32::System::Com::{CoDecrementMTAUsage, CO_MTA_USAGE_COOKIE};

        // SAFETY: this is the exact opaque cookie returned once by
        // CoIncrementMTAUsage and the reader is not Clone.
        let _ = unsafe { CoDecrementMTAUsage(CO_MTA_USAGE_COOKIE(self.mta_cookie as *mut _)) };
    }
}

#[cfg(windows)]
impl LocalAgentPlatformCredentialReader for WindowsLocalAgentCredentialReader {
    fn read(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        use windows::core::HSTRING;
        use windows::Security::Credentials::PasswordVault;

        let account = platform_credential_account_key(owner_user_id, reference)?;
        let vault =
            PasswordVault::new().map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        let credential = vault
            .Retrieve(&HSTRING::from(&self.resource), &HSTRING::from(account))
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        credential
            .RetrievePassword()
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        let secret = credential
            .Password()
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?
            .to_string()
            .into_bytes();
        if secret.is_empty() {
            Err(LocalAgentPlatformCredentialError::Unavailable)
        } else {
            Ok(Zeroizing::new(secret))
        }
    }
}

/// Reads the exact DPAPI files written by
/// `WindowsLocalAgentCredentialStore.SaveDeviceKeyAsync`.
#[cfg(windows)]
pub struct WindowsLocalAgentDeviceKeyReader {
    protected_key_directory: PathBuf,
}

#[cfg(windows)]
impl WindowsLocalAgentDeviceKeyReader {
    pub fn production() -> Result<Self, LocalAgentPlatformCredentialError> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or(LocalAgentPlatformCredentialError::Unavailable)?;
        if !local_app_data.is_absolute() {
            return Err(LocalAgentPlatformCredentialError::Unavailable);
        }
        let mut directory = local_app_data;
        for component in WINDOWS_PROTECTED_KEY_RELATIVE_DIRECTORY {
            if !component.is_empty() {
                directory.push(component);
            }
        }
        Self::for_directory(directory)
    }

    pub fn for_directory(
        protected_key_directory: impl Into<PathBuf>,
    ) -> Result<Self, LocalAgentPlatformCredentialError> {
        let protected_key_directory = protected_key_directory.into();
        if !protected_key_directory.is_absolute() {
            return Err(LocalAgentPlatformCredentialError::InvalidIdentity);
        }
        Ok(Self {
            protected_key_directory,
        })
    }

    fn protected_key_path(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<PathBuf, LocalAgentPlatformCredentialError> {
        let account = platform_credential_account_key(owner_user_id, reference)?;
        let digest = Sha256::digest(account.as_bytes());
        Ok(self
            .protected_key_directory
            .join(format!("{digest:x}{WINDOWS_DPAPI_FILE_SUFFIX}")))
    }
}

#[cfg(windows)]
impl fmt::Debug for WindowsLocalAgentDeviceKeyReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowsLocalAgentDeviceKeyReader")
            .field("protected_key_directory", &"[PRIVATE DIRECTORY]")
            .finish()
    }
}

#[cfg(windows)]
impl LocalAgentPlatformDeviceKeyReader for WindowsLocalAgentDeviceKeyReader {
    fn read_device_key(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        use std::fs::OpenOptions;
        use std::io::Read;
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
        };

        ensure_regular_non_reparse_directory(&self.protected_key_directory)?;
        let path = self.protected_key_path(owner_user_id, reference)?;
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        let metadata = file
            .metadata()
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            || metadata.len() == 0
            || metadata.len() > MAXIMUM_WINDOWS_PROTECTED_KEY_BYTES
        {
            return Err(LocalAgentPlatformCredentialError::Unavailable);
        }
        let capacity = usize::try_from(metadata.len())
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        let mut protected = Zeroizing::new(Vec::with_capacity(capacity));
        file.read_to_end(&mut protected)
            .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
        if protected.len() != capacity {
            return Err(LocalAgentPlatformCredentialError::Unavailable);
        }
        let account = platform_credential_account_key(owner_user_id, reference)?;
        let entropy = windows_dpapi_entropy(account.as_str());
        unprotect_windows_dpapi(protected.as_mut_slice(), entropy.as_slice())
    }
}

#[cfg(windows)]
fn windows_dpapi_entropy(account: &str) -> Zeroizing<Vec<u8>> {
    let mut input = Zeroizing::new(Vec::with_capacity(
        WINDOWS_DPAPI_ENTROPY_PREFIX.len() + account.len(),
    ));
    input.extend_from_slice(WINDOWS_DPAPI_ENTROPY_PREFIX);
    input.extend_from_slice(account.as_bytes());
    Zeroizing::new(Sha256::digest(input.as_slice()).to_vec())
}

#[cfg(windows)]
fn ensure_regular_non_reparse_directory(
    directory: &Path,
) -> Result<(), LocalAgentPlatformCredentialError> {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let metadata = directory
        .symlink_metadata()
        .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(LocalAgentPlatformCredentialError::Unavailable);
    }
    Ok(())
}

#[cfg(windows)]
fn unprotect_windows_dpapi(
    protected: &mut [u8],
    entropy: &[u8],
) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let protected_len = u32::try_from(protected.len())
        .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
    let entropy_len =
        u32::try_from(entropy.len()).map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
    let protected_blob = CRYPT_INTEGER_BLOB {
        cbData: protected_len,
        pbData: protected.as_mut_ptr(),
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy_len,
        pbData: entropy.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: both input blobs point to live slices for the duration of the
    // call. DPAPI allocates `output.pbData` with LocalAlloc; the guard below
    // zeroes and releases that buffer exactly once on every successful call.
    unsafe {
        CryptUnprotectData(
            &protected_blob,
            None,
            Some(&entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|_| LocalAgentPlatformCredentialError::Unavailable)?;
    if output.pbData.is_null() || output.cbData == 0 {
        if !output.pbData.is_null() {
            // SAFETY: DPAPI returned this LocalAlloc pointer.
            let _ = unsafe { LocalFree(Some(HLOCAL(output.pbData.cast()))) };
        }
        return Err(LocalAgentPlatformCredentialError::Unavailable);
    }
    let output_len = output.cbData as usize;
    // SAFETY: DPAPI reports `cbData` bytes at its non-null output pointer.
    let value =
        Zeroizing::new(unsafe { std::slice::from_raw_parts(output.pbData, output_len) }.to_vec());
    // SAFETY: the buffer is exclusively owned by this function until it is
    // passed back to LocalFree. Clearing it prevents plaintext key material
    // from remaining in the native heap.
    unsafe { std::ptr::write_bytes(output.pbData, 0, output_len) };
    // SAFETY: DPAPI returned this LocalAlloc pointer and it is freed once.
    let free_result = unsafe { LocalFree(Some(HLOCAL(output.pbData.cast()))) };
    if !free_result.0.is_null() {
        return Err(LocalAgentPlatformCredentialError::Unavailable);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_key_is_utf8_length_prefixed_and_collision_safe() {
        assert_eq!(
            platform_credential_account_key("用户-1", "postgres-1").unwrap(),
            "v1:8:用户-1postgres-1"
        );
        assert_ne!(
            platform_credential_account_key("account", "-secret").unwrap(),
            platform_credential_account_key("account-", "secret").unwrap()
        );
    }

    #[test]
    fn account_key_rejects_ambiguous_or_control_bearing_values() {
        for (owner, reference) in [
            ("", "secret"),
            (" user", "secret"),
            ("user", "secret\n"),
            ("user", ""),
        ] {
            assert_eq!(
                platform_credential_account_key(owner, reference).unwrap_err(),
                LocalAgentPlatformCredentialError::InvalidIdentity
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reader_uses_the_native_keychain_namespace() {
        use security_framework::passwords::{delete_generic_password, set_generic_password};

        let service = format!("com.chatos.local-agent.test.{}", std::process::id());
        let owner = "keychain-contract-user";
        let reference = "keychain-contract-secret";
        let account = platform_credential_account_key(owner, reference).unwrap();
        let _ = delete_generic_password(&service, &account);
        set_generic_password(&service, &account, b"temporary-contract-secret").unwrap();
        let reader = MacOsLocalAgentCredentialReader::for_service(service.clone());
        let secret = reader.read(owner, reference).unwrap();
        assert_eq!(secret.as_slice(), b"temporary-contract-secret");
        drop(secret);
        delete_generic_password(&service, &account).unwrap();
        assert_eq!(
            reader.read(owner, reference).unwrap_err(),
            LocalAgentPlatformCredentialError::Unavailable
        );
        assert!(!format!("{reader:?}").contains(service.as_str()));
    }

    #[cfg(windows)]
    #[test]
    fn windows_reader_decrypts_the_exact_current_user_dpapi_file_contract() {
        use std::fs;
        use windows::Win32::Foundation::{LocalFree, HLOCAL};
        use windows::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let directory = tempfile::tempdir().unwrap();
        let reader = WindowsLocalAgentDeviceKeyReader::for_directory(directory.path()).unwrap();
        let owner = "dpapi-contract-user";
        let reference = "provider-context-key";
        let account = platform_credential_account_key(owner, reference).unwrap();
        let entropy = windows_dpapi_entropy(account.as_str());
        let mut plaintext = Zeroizing::new(b"temporary-dpapi-contract-key".to_vec());
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(plaintext.len()).unwrap(),
            pbData: plaintext.as_mut_ptr(),
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(entropy.len()).unwrap(),
            pbData: entropy.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: the blobs point to live test buffers; the DPAPI allocation
        // is copied, cleared and released below.
        unsafe {
            CryptProtectData(
                &input,
                None,
                Some(&entropy_blob),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .unwrap();
        assert!(!output.pbData.is_null());
        let protected =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        unsafe { std::ptr::write_bytes(output.pbData, 0, output.cbData as usize) };
        assert!(unsafe { LocalFree(Some(HLOCAL(output.pbData.cast()))) }
            .0
            .is_null());
        fs::write(
            reader.protected_key_path(owner, reference).unwrap(),
            protected,
        )
        .unwrap();

        let restored = reader.read_device_key(owner, reference).unwrap();
        assert_eq!(restored.as_slice(), plaintext.as_slice());
        assert!(!format!("{reader:?}").contains(directory.path().to_string_lossy().as_ref()));
    }
}
