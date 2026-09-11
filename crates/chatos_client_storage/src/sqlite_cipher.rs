// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use crate::{StorageEncryptionKey, StorageError, StorageResult};

const PREFIX: &str = "chatos-encrypted-v1:";
const AAD: &[u8] = b"chatos-client-storage-record-v1";
const NONCE_LENGTH: usize = 12;

pub(crate) struct SqlitePayloadCipher {
    cipher: Aes256Gcm,
}

impl SqlitePayloadCipher {
    pub(crate) fn new(key: &StorageEncryptionKey) -> Self {
        Self {
            cipher: Aes256Gcm::new(key.expose().into()),
        }
    }

    pub(crate) fn encrypt(&self, plaintext: &str) -> StorageResult<String> {
        let mut nonce_bytes = [0_u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: AAD,
                },
            )
            .map_err(|_| StorageError::InvalidData {
                reason: "SQLite payload encryption failed".to_string(),
            })?;
        let mut envelope = Vec::with_capacity(NONCE_LENGTH + ciphertext.len());
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&ciphertext);
        Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(envelope)))
    }

    pub(crate) fn decrypt(&self, envelope: &str) -> StorageResult<String> {
        let encoded = envelope
            .strip_prefix(PREFIX)
            .ok_or_else(|| StorageError::InvalidData {
                reason: "unencrypted SQLite business payload was rejected".to_string(),
            })?;
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| StorageError::InvalidData {
                reason: "SQLite encrypted payload is malformed".to_string(),
            })?;
        if bytes.len() <= NONCE_LENGTH {
            return Err(StorageError::InvalidData {
                reason: "SQLite encrypted payload is truncated".to_string(),
            });
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LENGTH);
        let nonce =
            <[u8; NONCE_LENGTH]>::try_from(nonce).map_err(|_| StorageError::InvalidData {
                reason: "SQLite encrypted payload has an invalid nonce".to_string(),
            })?;
        let nonce = Nonce::from(nonce);
        let plaintext = self
            .cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: AAD,
                },
            )
            .map_err(|_| StorageError::InvalidData {
                reason: "SQLite encrypted payload authentication failed".to_string(),
            })?;
        String::from_utf8(plaintext).map_err(|_| StorageError::InvalidData {
            reason: "SQLite decrypted payload is not UTF-8".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_is_authenticated_and_randomized() {
        let cipher = SqlitePayloadCipher::new(&StorageEncryptionKey::new([7; 32]));
        let first = cipher.encrypt("sensitive payload").unwrap();
        let second = cipher.encrypt("sensitive payload").unwrap();

        assert_ne!(first, second);
        assert!(!first.contains("sensitive payload"));
        assert_eq!(cipher.decrypt(&first).unwrap(), "sensitive payload");

        let mut damaged = first.into_bytes();
        let last = damaged.last_mut().unwrap();
        *last = if *last == b'A' { b'B' } else { b'A' };
        assert!(matches!(
            cipher.decrypt(std::str::from_utf8(&damaged).unwrap()),
            Err(StorageError::InvalidData { .. })
        ));
    }

    #[test]
    fn plaintext_legacy_payloads_are_not_accepted() {
        let cipher = SqlitePayloadCipher::new(&StorageEncryptionKey::new([7; 32]));
        assert!(matches!(
            cipher.decrypt("{\"plaintext\":true}"),
            Err(StorageError::InvalidData { .. })
        ));
    }
}
