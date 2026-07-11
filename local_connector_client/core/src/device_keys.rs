// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

use crate::LocalState;

const PUBLIC_KEY_PREFIX: &str = "ed25519:";
const KEY_FILE_NAME: &str = "device-signing-key.bin";

pub(crate) fn ensure_device_keypair(
    state_path: &Path,
    state: &mut LocalState,
    requested_public_key: Option<&str>,
) -> Result<String> {
    if let Some(requested_public_key) = requested_public_key {
        ensure_supported_public_key(requested_public_key)?;
    }

    if let Some(pkcs8) = load_private_key(state_path)? {
        let public_key = public_key_from_pkcs8(pkcs8.as_slice())?;
        if requested_public_key
            .map(|requested| requested == public_key)
            .unwrap_or(true)
        {
            apply_public_key(state, public_key.clone());
            return Ok(public_key);
        }
    }

    if requested_public_key.is_some() {
        return Err(anyhow!(
            "LOCAL_CONNECTOR_PUBLIC_KEY was provided, but the matching local private key is unavailable"
        ));
    }

    let (public_key, pkcs8) = generate_keypair()?;
    save_private_key(state_path, pkcs8.as_slice())?;
    state.device_id = None;
    state.device_public_key = Some(public_key.clone());
    Ok(public_key)
}

pub(crate) fn sign_device_message(
    state_path: &Path,
    public_key: &str,
    payload: &[u8],
) -> Result<String> {
    ensure_supported_public_key(public_key)?;
    let pkcs8 = load_private_key(state_path)?
        .ok_or_else(|| anyhow!("local connector device private key is unavailable"))?;
    let derived_public_key = public_key_from_pkcs8(pkcs8.as_slice())?;
    if derived_public_key != public_key {
        return Err(anyhow!(
            "local connector device private key does not match the registered public key"
        ));
    }
    let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_slice())
        .map_err(|_| anyhow!("load local connector device private key failed"))?;
    Ok(URL_SAFE_NO_PAD.encode(keypair.sign(payload).as_ref()))
}

fn ensure_supported_public_key(value: &str) -> Result<()> {
    public_key_bytes(value)
        .map(|_| ())
        .ok_or_else(|| anyhow!("local connector device public key must be an ed25519 key"))
}

fn public_key_bytes(value: &str) -> Option<Vec<u8>> {
    let encoded = value.trim().strip_prefix(PUBLIC_KEY_PREFIX)?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).ok()?;
    (bytes.len() == 32).then_some(bytes)
}

fn apply_public_key(state: &mut LocalState, public_key: String) {
    if state.device_public_key.as_deref() != Some(public_key.as_str()) {
        state.device_id = None;
        state.device_public_key = Some(public_key);
    }
}

fn generate_keypair() -> Result<(String, Vec<u8>)> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| anyhow!("generate local connector device key failed"))?;
    let public_key = public_key_from_pkcs8(pkcs8.as_ref())?;
    Ok((public_key, pkcs8.as_ref().to_vec()))
}

fn public_key_from_pkcs8(pkcs8: &[u8]) -> Result<String> {
    let keypair = Ed25519KeyPair::from_pkcs8(pkcs8)
        .map_err(|_| anyhow!("load local connector device key failed"))?;
    Ok(format!(
        "{PUBLIC_KEY_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
    ))
}

fn load_private_key(state_path: &Path) -> Result<Option<Vec<u8>>> {
    let path = private_key_path(state_path);
    if !path.is_file() {
        return Ok(None);
    }
    secret_store::load(path.as_path())
        .with_context(|| format!("load local connector device key {}", path.display()))
}

fn save_private_key(state_path: &Path, pkcs8: &[u8]) -> Result<()> {
    let path = private_key_path(state_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create local connector key dir {}", parent.display()))?;
    }
    secret_store::save(path.as_path(), pkcs8)
        .with_context(|| format!("save local connector device key {}", path.display()))
}

fn private_key_path(state_path: &Path) -> PathBuf {
    state_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(KEY_FILE_NAME)
}

mod secret_store {
    use super::*;

    #[cfg(windows)]
    const DPAPI_MAGIC: &[u8] = b"dpapi-v1\n";

    #[cfg(windows)]
    pub(super) fn load(path: &Path) -> Result<Option<Vec<u8>>> {
        let content = fs::read(path)?;
        if let Some(encrypted) = content.strip_prefix(DPAPI_MAGIC) {
            return dpapi_unprotect(encrypted).map(Some);
        }
        Ok(Some(content))
    }

    #[cfg(windows)]
    pub(super) fn save(path: &Path, value: &[u8]) -> Result<()> {
        let encrypted = dpapi_protect(value)?;
        let mut content = DPAPI_MAGIC.to_vec();
        content.extend_from_slice(encrypted.as_slice());
        fs::write(path, content)?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub(super) fn load(path: &Path) -> Result<Option<Vec<u8>>> {
        let account = keychain_account(path);
        let output = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-a",
                account.as_str(),
                "-s",
                "ChatOS Local Connector Device Key",
                "-w",
            ])
            .output()?;
        if !output.status.success() {
            return Ok(None);
        }
        let value = String::from_utf8(output.stdout)?;
        URL_SAFE_NO_PAD
            .decode(value.trim().as_bytes())
            .map(Some)
            .map_err(|err| {
                anyhow!("decode macOS Keychain local connector device key failed: {err}")
            })
    }

    #[cfg(target_os = "macos")]
    pub(super) fn save(path: &Path, value: &[u8]) -> Result<()> {
        let account = keychain_account(path);
        let encoded = URL_SAFE_NO_PAD.encode(value);
        let status = std::process::Command::new("security")
            .args([
                "add-generic-password",
                "-a",
                account.as_str(),
                "-s",
                "ChatOS Local Connector Device Key",
                "-w",
                encoded.as_str(),
                "-U",
            ])
            .status()?;
        if !status.success() {
            return Err(anyhow!(
                "store local connector device key in macOS Keychain failed"
            ));
        }
        Ok(())
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    pub(super) fn load(path: &Path) -> Result<Option<Vec<u8>>> {
        Ok(Some(fs::read(path)?))
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    pub(super) fn save(path: &Path, value: &[u8]) -> Result<()> {
        fs::write(path, value)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn keychain_account(path: &Path) -> String {
        let digest = sha2::Sha256::digest(path.to_string_lossy().as_bytes());
        format!("chatos-local-connector-{}", hex::encode(digest))
    }

    #[cfg(windows)]
    fn dpapi_protect(value: &[u8]) -> Result<Vec<u8>> {
        use std::ptr::{null, null_mut};
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let mut input = CRYPT_INTEGER_BLOB {
            cbData: value.len() as u32,
            pbData: value.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &mut input,
                null(),
                null(),
                null_mut(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let protected =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        unsafe {
            LocalFree(output.pbData as _);
        }
        Ok(protected)
    }

    #[cfg(windows)]
    fn dpapi_unprotect(value: &[u8]) -> Result<Vec<u8>> {
        use std::ptr::{null, null_mut};
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let mut input = CRYPT_INTEGER_BLOB {
            cbData: value.len() as u32,
            pbData: value.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &mut input,
                null_mut(),
                null(),
                null_mut(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let unprotected =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        unsafe {
            LocalFree(output.pbData as _);
        }
        Ok(unprotected)
    }
}
