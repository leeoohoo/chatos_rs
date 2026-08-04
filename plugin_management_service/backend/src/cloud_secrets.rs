// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const ENVELOPE_VERSION: &str = "v1";
const NONCE_BYTES: usize = 12;

#[derive(Clone)]
pub(crate) struct CloudSecretCipher {
    key: [u8; 32],
}

impl CloudSecretCipher {
    pub(crate) fn new(secret: &str) -> Result<Self, String> {
        let secret = secret.trim();
        if secret.is_empty() {
            return Err("Plugin cloud credential encryption secret cannot be empty".to_string());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"chatos.plugin.cloud-credential-key.v1\n");
        hasher.update(secret.as_bytes());
        let digest = hasher.finalize();
        let mut key = [0_u8; 32];
        key.copy_from_slice(digest.as_slice());
        Ok(Self { key })
    }

    pub(crate) fn encrypt(&self, plain: &str, aad: &str) -> Result<String, String> {
        let mut nonce = [0_u8; NONCE_BYTES];
        rand::fill(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|error| {
            format!("initialize Plugin cloud credential cipher failed: {error}")
        })?;
        let nonce_ref = Nonce::try_from(nonce.as_slice())
            .map_err(|error| format!("initialize Plugin cloud credential nonce failed: {error}"))?;
        let encrypted = cipher
            .encrypt(
                &nonce_ref,
                Payload {
                    msg: plain.as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|error| format!("encrypt Plugin cloud credential failed: {error}"))?;
        Ok(format!(
            "{ENVELOPE_VERSION}.{}.{}",
            URL_SAFE_NO_PAD.encode(nonce),
            URL_SAFE_NO_PAD.encode(encrypted)
        ))
    }

    pub(crate) fn decrypt(&self, envelope: &str, aad: &str) -> Result<Zeroizing<String>, String> {
        let mut parts = envelope.split('.');
        let version = parts.next().unwrap_or_default();
        let encoded_nonce = parts.next().unwrap_or_default();
        let encoded_ciphertext = parts.next().unwrap_or_default();
        if version != ENVELOPE_VERSION || parts.next().is_some() {
            return Err("Plugin cloud credential envelope is invalid".to_string());
        }
        let nonce = URL_SAFE_NO_PAD
            .decode(encoded_nonce)
            .map_err(|_| "Plugin cloud credential nonce is invalid".to_string())?;
        if nonce.len() != NONCE_BYTES {
            return Err("Plugin cloud credential nonce length is invalid".to_string());
        }
        let ciphertext = URL_SAFE_NO_PAD
            .decode(encoded_ciphertext)
            .map_err(|_| "Plugin cloud credential ciphertext is invalid".to_string())?;
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|error| {
            format!("initialize Plugin cloud credential cipher failed: {error}")
        })?;
        let nonce_ref = Nonce::try_from(nonce.as_slice())
            .map_err(|error| format!("initialize Plugin cloud credential nonce failed: {error}"))?;
        let plain = cipher
            .decrypt(
                &nonce_ref,
                Payload {
                    msg: ciphertext.as_slice(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| "decrypt Plugin cloud credential failed".to_string())?;
        String::from_utf8(plain)
            .map(Zeroizing::new)
            .map_err(|_| "Plugin cloud credential is not UTF-8".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::CloudSecretCipher;

    #[test]
    fn envelope_is_bound_to_its_scope() {
        let cipher = CloudSecretCipher::new("test-cloud-secret").unwrap();
        let encrypted = cipher.encrypt("token-value", "scope-a").unwrap();
        assert_eq!(
            cipher
                .decrypt(encrypted.as_str(), "scope-a")
                .unwrap()
                .as_str(),
            "token-value"
        );
        assert!(cipher.decrypt(encrypted.as_str(), "scope-b").is_err());
        assert!(!encrypted.contains("token-value"));
    }
}
