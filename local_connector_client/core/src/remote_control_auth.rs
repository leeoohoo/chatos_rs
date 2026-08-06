// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use tokio::sync::Mutex;

use crate::config::RemoteControlTrustConfig;
use crate::device_keys::verify_device_message_signature;
use crate::relay::RelayRequest;
use crate::relay_signature::relay_request_signature_payload;

#[derive(Clone)]
pub(crate) struct RemoteControlVerifier {
    config: Arc<RwLock<RemoteControlVerifierConfig>>,
    seen_nonces: Arc<Mutex<HashMap<String, i64>>>,
}

#[derive(Clone, PartialEq, Eq)]
struct RemoteControlVerifierConfig {
    require_signed_messages: bool,
    signature_max_skew_seconds: i64,
    trusted_relay_public_keys: HashMap<String, String>,
}

impl RemoteControlVerifier {
    pub(crate) fn from_state(state: &crate::LocalState) -> anyhow::Result<Self> {
        Ok(Self::new(RemoteControlTrustConfig::from_state(state)?))
    }

    pub(crate) fn new(config: RemoteControlTrustConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(RemoteControlVerifierConfig::from(config))),
            seen_nonces: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn reload_from_state(
        &self,
        state: &crate::LocalState,
    ) -> anyhow::Result<bool> {
        self.reload(RemoteControlTrustConfig::from_state(state)?)
            .await
    }

    pub(crate) async fn reload(&self, config: RemoteControlTrustConfig) -> anyhow::Result<bool> {
        let next = RemoteControlVerifierConfig::from(config);
        let changed = {
            let mut current = self
                .config
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        };
        if changed {
            self.seen_nonces.lock().await.clear();
        }
        Ok(changed)
    }

    pub(crate) async fn verify(&self, request: &RelayRequest) -> Result<(), String> {
        let config = self
            .config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let has_signature_fields = request.platform_signature.is_some()
            || request.platform_signature_key_id.is_some()
            || request.platform_signature_alg.is_some()
            || request.platform_timestamp.is_some()
            || request.platform_nonce.is_some();
        if !has_signature_fields {
            if config.require_signed_messages {
                return Err("relay platform signature is required".to_string());
            }
            return Ok(());
        }

        let algorithm = required_text(
            request.platform_signature_alg.as_deref(),
            "relay platform_signature_alg",
        )?;
        if algorithm != "ed25519" {
            return Err("relay platform signature algorithm is not supported".to_string());
        }

        let key_id = required_text(
            request.platform_signature_key_id.as_deref(),
            "relay platform_signature_key_id",
        )?;
        let public_key = self
            .trusted_relay_public_keys(&config)
            .get(key_id.as_str())
            .ok_or_else(|| "relay platform signing key id is not trusted".to_string())?;

        let timestamp = request
            .platform_timestamp
            .ok_or_else(|| "relay platform_timestamp is required".to_string())?;
        let now = Utc::now().timestamp();
        if now.saturating_sub(timestamp).abs() > config.signature_max_skew_seconds {
            return Err(
                "relay platform signature timestamp is outside the allowed window".to_string(),
            );
        }

        let nonce = required_text(request.platform_nonce.as_deref(), "relay platform_nonce")?;
        if nonce.len() < 16 || nonce.len() > 128 {
            return Err("relay platform signature nonce is invalid".to_string());
        }

        let signature = required_text(
            request.platform_signature.as_deref(),
            "relay platform_signature",
        )?;
        let payload = relay_request_signature_payload(request)?;
        verify_device_message_signature(
            public_key.as_str(),
            payload.as_slice(),
            signature.as_str(),
        )
        .map_err(|err| format!("relay platform signature verification failed: {err}"))?;
        if !self
            .consume_nonce(
                key_id.as_str(),
                nonce.as_str(),
                now,
                config.signature_max_skew_seconds,
            )
            .await
        {
            return Err("relay platform signature nonce was already used".to_string());
        }
        Ok(())
    }

    fn trusted_relay_public_keys<'a>(
        &self,
        config: &'a RemoteControlVerifierConfig,
    ) -> &'a HashMap<String, String> {
        &config.trusted_relay_public_keys
    }

    async fn consume_nonce(
        &self,
        key_id: &str,
        nonce: &str,
        now: i64,
        signature_max_skew_seconds: i64,
    ) -> bool {
        let expires_at = now.saturating_add(signature_max_skew_seconds);
        let min_expires_at = now.saturating_sub(signature_max_skew_seconds);
        let cache_key = format!("{key_id}:{nonce}");
        let mut seen_nonces = self.seen_nonces.lock().await;
        seen_nonces.retain(|_, expires_at| *expires_at >= min_expires_at);
        if seen_nonces.contains_key(cache_key.as_str()) {
            return false;
        }
        seen_nonces.insert(cache_key, expires_at);
        true
    }
}

impl From<RemoteControlTrustConfig> for RemoteControlVerifierConfig {
    fn from(config: RemoteControlTrustConfig) -> Self {
        Self {
            require_signed_messages: config.require_signed_messages,
            signature_max_skew_seconds: config.signature_max_skew.as_secs() as i64,
            trusted_relay_public_keys: config.trusted_relay_public_keys,
        }
    }
}

fn required_text(value: Option<&str>, label: &str) -> Result<String, String> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} is required"))?;
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use super::*;

    fn verifier_and_request() -> (RemoteControlVerifier, RelayRequest, Ed25519KeyPair) {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("generate key");
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("load keypair");
        let public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
        );
        let verifier = RemoteControlVerifier::new(RemoteControlTrustConfig {
            require_signed_messages: true,
            signature_max_skew: std::time::Duration::from_secs(300),
            trusted_relay_public_keys: HashMap::from([("relay-key-1".to_string(), public_key)]),
        });
        let request = RelayRequest {
            _message_type: "plugin_execute_request".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("owner-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/plugins/execute".to_string()),
            headers: HashMap::from([("x-demo".to_string(), "1".to_string())])
                .into_iter()
                .collect(),
            body: serde_json::json!({"tool":"browser","args":{"b":2,"a":1}}),
            platform_signature: None,
            platform_signature_key_id: Some("relay-key-1".to_string()),
            platform_signature_alg: Some("ed25519".to_string()),
            platform_timestamp: Some(Utc::now().timestamp()),
            platform_nonce: Some("12345678-1234-1234-1234-1234567890ab".to_string()),
        };
        (verifier, request, keypair)
    }

    fn sign_request(request: &mut RelayRequest, keypair: &Ed25519KeyPair) {
        let payload = relay_request_signature_payload(request).expect("payload");
        request.platform_signature =
            Some(URL_SAFE_NO_PAD.encode(keypair.sign(payload.as_slice()).as_ref()));
    }

    #[tokio::test]
    async fn accepts_valid_signed_remote_control_request() {
        let (verifier, mut request, keypair) = verifier_and_request();
        sign_request(&mut request, &keypair);
        verifier.verify(&request).await.expect("verify request");
    }

    #[tokio::test]
    async fn rejects_tampered_request_body() {
        let (verifier, mut request, keypair) = verifier_and_request();
        sign_request(&mut request, &keypair);
        let original_body = request.body.clone();
        request.body = serde_json::json!({"tool":"browser","args":{"a":1,"b":3}});
        let error = verifier
            .verify(&request)
            .await
            .expect_err("tampered request");
        assert!(error.contains("verification failed"));

        request.body = original_body;
        verifier
            .verify(&request)
            .await
            .expect("invalid signature must not consume the nonce");
    }

    #[tokio::test]
    async fn rejects_replayed_nonce() {
        let (verifier, mut request, keypair) = verifier_and_request();
        sign_request(&mut request, &keypair);
        verifier.verify(&request).await.expect("first verify");
        let error = verifier
            .verify(&request)
            .await
            .expect_err("replayed request");
        assert!(error.contains("already used"));
    }

    #[tokio::test]
    async fn rejects_unsigned_request_when_signatures_are_required() {
        let (verifier, request, _keypair) = verifier_and_request();
        let error = verifier
            .verify(&request)
            .await
            .expect_err("unsigned request should be rejected");
        assert!(error.contains("relay platform_signature"));
    }

    #[tokio::test]
    async fn reload_replaces_trusted_keys_and_clears_nonce_cache() {
        let (verifier, mut request, first_keypair) = verifier_and_request();
        sign_request(&mut request, &first_keypair);
        verifier.verify(&request).await.expect("first verify");

        let next_pkcs8 =
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("generate second key");
        let next_keypair =
            Ed25519KeyPair::from_pkcs8(next_pkcs8.as_ref()).expect("load second keypair");
        let next_public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(next_keypair.public_key().as_ref())
        );
        verifier
            .reload(RemoteControlTrustConfig {
                require_signed_messages: true,
                signature_max_skew: std::time::Duration::from_secs(300),
                trusted_relay_public_keys: HashMap::from([(
                    "relay-key-2".to_string(),
                    next_public_key,
                )]),
            })
            .await
            .expect("reload verifier");

        let mut rotated_request = request.clone();
        rotated_request.platform_signature = None;
        rotated_request.platform_signature_key_id = Some("relay-key-2".to_string());
        sign_request(&mut rotated_request, &next_keypair);
        verifier
            .verify(&rotated_request)
            .await
            .expect("verify rotated request");
    }
}
