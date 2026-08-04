// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};

const SECRET_PREFIX: &str = "enc:v1:";
const NONCE_SIZE: usize = 12;
const PREVIOUS_SECRET_KEYS_ENV: &str = "USER_SERVICE_PREVIOUS_SECRET_KEYS";

fn load_secret_material() -> Result<String, String> {
    std::env::var("USER_SERVICE_SECRET_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "USER_SERVICE_SECRET_KEY is required from configuration center".to_string())
}

fn derive_secret_key(secret_material: &str) -> [u8; 32] {
    let digest = Sha256::digest(secret_material.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest[..32]);
    key
}

fn load_previous_secret_materials() -> Vec<String> {
    std::env::var(PREVIOUS_SECRET_KEYS_ENV)
        .ok()
        .map(|value| {
            value
                .split([',', '\n', ';'])
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn load_decryption_keys() -> Result<Vec<[u8; 32]>, String> {
    let mut materials = Vec::new();
    let primary = load_secret_material()?;
    materials.push(primary.clone());
    for candidate in load_previous_secret_materials() {
        if candidate != primary {
            materials.push(candidate);
        }
    }
    Ok(materials
        .into_iter()
        .map(|material| derive_secret_key(material.as_str()))
        .collect())
}

pub fn encrypt_secret(plain_text: &str) -> Result<String, String> {
    if is_secret_encrypted(plain_text) {
        return Err(
            "encrypt secret failed: refusing to encrypt an already encrypted secret".to_string(),
        );
    }
    let mut nonce = [0u8; NONCE_SIZE];
    rand::fill(&mut nonce);
    let primary_key = derive_secret_key(load_secret_material()?.as_str());
    let cipher = Aes256Gcm::new_from_slice(&primary_key)
        .map_err(|err| format!("encrypt secret failed: invalid key: {err}"))?;
    let nonce_ref = Nonce::try_from(nonce.as_slice())
        .map_err(|err| format!("encrypt secret failed: invalid nonce: {err}"))?;
    let encrypted = cipher
        .encrypt(&nonce_ref, plain_text.as_bytes())
        .map_err(|err| format!("encrypt secret failed: {err}"))?;
    Ok(format!(
        "{}{}:{}",
        SECRET_PREFIX,
        STANDARD.encode(nonce),
        STANDARD.encode(encrypted)
    ))
}

pub fn is_secret_encrypted(value: &str) -> bool {
    value.starts_with(SECRET_PREFIX)
}

pub fn decrypt_secret(value: &str) -> Result<String, String> {
    if !is_secret_encrypted(value) {
        return Ok(value.to_string());
    }

    let payload = &value[SECRET_PREFIX.len()..];
    let mut parts = payload.splitn(2, ':');
    let nonce_b64 = parts
        .next()
        .ok_or_else(|| "decrypt secret failed: missing nonce".to_string())?;
    let data_b64 = parts
        .next()
        .ok_or_else(|| "decrypt secret failed: missing ciphertext".to_string())?;

    let nonce = STANDARD
        .decode(nonce_b64)
        .map_err(|err| format!("decrypt secret failed: invalid nonce: {err}"))?;
    if nonce.len() != NONCE_SIZE {
        return Err("decrypt secret failed: invalid nonce size".to_string());
    }
    let encrypted = STANDARD
        .decode(data_b64)
        .map_err(|err| format!("decrypt secret failed: invalid ciphertext: {err}"))?;
    let nonce_ref = Nonce::try_from(nonce.as_slice())
        .map_err(|err| format!("decrypt secret failed: invalid nonce: {err}"))?;
    for key in load_decryption_keys()? {
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|err| format!("decrypt secret failed: invalid key: {err}"))?;
        match cipher.decrypt(&nonce_ref, encrypted.as_ref()) {
            Ok(plain) => {
                return String::from_utf8(plain)
                    .map_err(|err| format!("decrypt secret failed: invalid utf8: {err}"));
            }
            Err(_) => continue,
        }
    }
    Err("decrypt secret failed: key mismatch or corrupted data".to_string())
}

pub fn encrypt_optional_secret(value: Option<String>) -> Result<Option<String>, String> {
    value.map(|item| encrypt_secret(item.as_str())).transpose()
}

pub fn decrypt_optional_secret(value: Option<String>) -> Result<Option<String>, String> {
    value.map(|item| decrypt_secret(item.as_str())).transpose()
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::{decrypt_secret, encrypt_secret, is_secret_encrypted, PREVIOUS_SECRET_KEYS_ENV};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn encrypt_and_decrypt_roundtrip() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("USER_SERVICE_SECRET_KEY", "user-service-test-secret");
        std::env::remove_var(PREVIOUS_SECRET_KEYS_ENV);
        let encrypted = encrypt_secret("secret-123").expect("encrypt");
        assert!(is_secret_encrypted(encrypted.as_str()));
        let decrypted = decrypt_secret(encrypted.as_str()).expect("decrypt");
        assert_eq!(decrypted, "secret-123");
        std::env::remove_var("USER_SERVICE_SECRET_KEY");
        std::env::remove_var(PREVIOUS_SECRET_KEYS_ENV);
    }

    #[test]
    fn decrypts_secret_with_previous_key_ring() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var("USER_SERVICE_SECRET_KEY", "legacy-user-service-secret");
        std::env::remove_var(PREVIOUS_SECRET_KEYS_ENV);
        let encrypted = encrypt_secret("legacy-token").expect("encrypt with legacy key");

        std::env::set_var("USER_SERVICE_SECRET_KEY", "current-user-service-secret");
        std::env::set_var(
            PREVIOUS_SECRET_KEYS_ENV,
            "legacy-user-service-secret, older-unused-secret",
        );
        let decrypted = decrypt_secret(encrypted.as_str()).expect("decrypt with previous key");
        assert_eq!(decrypted, "legacy-token");

        std::env::remove_var("USER_SERVICE_SECRET_KEY");
        std::env::remove_var(PREVIOUS_SECRET_KEYS_ENV);
    }
}
