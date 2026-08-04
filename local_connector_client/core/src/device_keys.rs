// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use sha2::{Digest, Sha256};

use crate::secure_storage::SecureStorage;
use crate::LocalState;

const PUBLIC_KEY_PREFIX: &str = "ed25519:";
const KEY_FILE_NAME: &str = "device-signing-key.bin";
const DEVICE_KEY_SERVICE: &str = "Chat OS Local Connector Device Key";

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

pub(crate) fn verify_device_message_signature(
    public_key: &str,
    payload: &[u8],
    signature: &str,
) -> Result<()> {
    let public_key = public_key_bytes(public_key)
        .ok_or_else(|| anyhow!("local connector device public key must be an ed25519 key"))?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature.trim().as_bytes())
        .map_err(|err| anyhow!("decode local connector device signature failed: {err}"))?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(payload, signature.as_slice())
        .map_err(|_| anyhow!("local connector device signature verification failed"))
}

pub(crate) fn ensure_supported_public_key(value: &str) -> Result<()> {
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
    SecureStorage::platform(DEVICE_KEY_SERVICE)
        .load(device_key_account(path.as_path()).as_str(), path.as_path())
        .with_context(|| format!("load local connector device key {}", path.display()))
}

fn save_private_key(state_path: &Path, pkcs8: &[u8]) -> Result<()> {
    let path = private_key_path(state_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create local connector key dir {}", parent.display()))?;
    }
    SecureStorage::platform(DEVICE_KEY_SERVICE)
        .save(
            device_key_account(path.as_path()).as_str(),
            path.as_path(),
            pkcs8,
        )
        .with_context(|| format!("save local connector device key {}", path.display()))
}

fn private_key_path(state_path: &Path) -> PathBuf {
    state_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(KEY_FILE_NAME)
}

fn device_key_account(path: &Path) -> String {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    format!("chatos-local-connector-{}", hex::encode(digest))
}
