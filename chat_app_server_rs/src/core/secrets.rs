use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

const SECRET_PREFIX: &str = "enc:v1:";
const NONCE_SIZE: usize = 12;

static SECRET_KEY: OnceCell<[u8; 32]> = OnceCell::new();

fn load_secret_material() -> String {
    std::env::var("CHATOS_REMOTE_SECRET_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            std::env::var("AUTH_JWT_SECRET")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .unwrap_or_else(|| "dev-only-change-me-please".to_string())
}

fn secret_key() -> &'static [u8; 32] {
    SECRET_KEY.get_or_init(|| {
        let material = load_secret_material();
        let digest = Sha256::digest(material.as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest[..32]);
        key
    })
}

pub fn encrypt_secret(plain_text: &str) -> Result<String, String> {
    let mut nonce = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(secret_key()));
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), plain_text.as_bytes())
        .map_err(|e| format!("加密敏感字段失败: {e}"))?;
    Ok(format!(
        "{}{}:{}",
        SECRET_PREFIX,
        STANDARD.encode(nonce),
        STANDARD.encode(encrypted)
    ))
}

pub fn decrypt_secret(value: &str) -> Result<String, String> {
    if !value.starts_with(SECRET_PREFIX) {
        return Ok(value.to_string());
    }
    let payload = &value[SECRET_PREFIX.len()..];
    let mut parts = payload.splitn(2, ':');
    let nonce_b64 = parts
        .next()
        .ok_or_else(|| "敏感字段解密失败: nonce 缺失".to_string())?;
    let data_b64 = parts
        .next()
        .ok_or_else(|| "敏感字段解密失败: 密文缺失".to_string())?;

    let nonce = STANDARD
        .decode(nonce_b64)
        .map_err(|e| format!("敏感字段解密失败: nonce 无效: {e}"))?;
    if nonce.len() != NONCE_SIZE {
        return Err("敏感字段解密失败: nonce 长度无效".to_string());
    }
    let encrypted = STANDARD
        .decode(data_b64)
        .map_err(|e| format!("敏感字段解密失败: 密文无效: {e}"))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(secret_key()));
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce), encrypted.as_ref())
        .map_err(|_| "敏感字段解密失败: 密钥不匹配或数据损坏".to_string())?;
    String::from_utf8(plain).map_err(|e| format!("敏感字段解密失败: 文本无效: {e}"))
}

pub fn encrypt_optional_secret(value: Option<String>) -> Result<Option<String>, String> {
    value.map(|v| encrypt_secret(v.as_str())).transpose()
}

pub fn decrypt_optional_secret(value: Option<String>) -> Result<Option<String>, String> {
    value.map(|v| decrypt_secret(v.as_str())).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_and_decrypt_roundtrip() {
        let text = "secret-password";
        let encrypted = encrypt_secret(text).expect("encrypt");
        assert!(encrypted.starts_with(SECRET_PREFIX));
        let decrypted = decrypt_secret(encrypted.as_str()).expect("decrypt");
        assert_eq!(decrypted, text);
    }

    #[test]
    fn passthrough_plain_text() {
        let raw = "plain-text";
        let decrypted = decrypt_secret(raw).expect("decrypt plain");
        assert_eq!(decrypted, raw);
    }
}
