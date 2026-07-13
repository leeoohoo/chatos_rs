// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use once_cell::sync::OnceCell;
use sha2::{Digest, Sha256};

const SECRET_PREFIX: &str = "enc:v1:";
const NONCE_SIZE: usize = 12;

static SECRET_KEY: OnceCell<[u8; 32]> = OnceCell::new();

fn load_secret_material() -> String {
    std::env::var("USER_SERVICE_SECRET_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("USER_SERVICE_JWT_SECRET")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "change_me_user_service_secret".to_string())
}

fn secret_key() -> &'static [u8; 32] {
    SECRET_KEY.get_or_init(|| {
        let digest = Sha256::digest(load_secret_material().as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest[..32]);
        key
    })
}

pub fn encrypt_secret(plain_text: &str) -> Result<String, String> {
    if is_secret_encrypted(plain_text) {
        return Err(
            "encrypt secret failed: refusing to encrypt an already encrypted secret".to_string(),
        );
    }
    let mut nonce = [0u8; NONCE_SIZE];
    rand::fill(&mut nonce);
    let cipher = Aes256Gcm::new_from_slice(secret_key())
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

    let cipher = Aes256Gcm::new_from_slice(secret_key())
        .map_err(|err| format!("decrypt secret failed: invalid key: {err}"))?;
    let nonce_ref = Nonce::try_from(nonce.as_slice())
        .map_err(|err| format!("decrypt secret failed: invalid nonce: {err}"))?;
    let plain = cipher
        .decrypt(&nonce_ref, encrypted.as_ref())
        .map_err(|_| "decrypt secret failed: key mismatch or corrupted data".to_string())?;
    String::from_utf8(plain).map_err(|err| format!("decrypt secret failed: invalid utf8: {err}"))
}

pub fn encrypt_optional_secret(value: Option<String>) -> Result<Option<String>, String> {
    value.map(|item| encrypt_secret(item.as_str())).transpose()
}

pub fn decrypt_optional_secret(value: Option<String>) -> Result<Option<String>, String> {
    value.map(|item| decrypt_secret(item.as_str())).transpose()
}

#[cfg(test)]
mod tests {
    use super::{decrypt_secret, encrypt_secret, is_secret_encrypted};

    #[test]
    fn encrypt_and_decrypt_roundtrip() {
        std::env::set_var("USER_SERVICE_SECRET_KEY", "user-service-test-secret");
        let encrypted = encrypt_secret("secret-123").expect("encrypt");
        assert!(is_secret_encrypted(encrypted.as_str()));
        let decrypted = decrypt_secret(encrypted.as_str()).expect("decrypt");
        assert_eq!(decrypted, "secret-123");
        std::env::remove_var("USER_SERVICE_SECRET_KEY");
    }
}
