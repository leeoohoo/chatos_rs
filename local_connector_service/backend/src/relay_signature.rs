// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::Utc;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::Value;
use uuid::Uuid;

use chatos_agent::RemoteControlTrustConfigBundle;

use crate::managed_config::PlatformRelaySigningConfig;
use crate::relay::RelayRequest;

pub(crate) struct PlatformRelaySigner {
    key_id: String,
    keypair: Ed25519KeyPair,
    public_key: String,
}

impl std::fmt::Debug for PlatformRelaySigner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlatformRelaySigner")
            .field("key_id", &self.key_id)
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

impl PlatformRelaySigner {
    pub(crate) fn load(config: &PlatformRelaySigningConfig) -> Result<Arc<Self>, String> {
        let key_id = config.key_id.trim().to_string();
        if key_id.is_empty() {
            return Err("relay signing key id must not be empty".to_string());
        }
        let key_path = config.key_path.as_path();
        let key_bytes = read_private_key_file("relay signing key", key_path, 16 * 1024)?;
        let keypair = Ed25519KeyPair::from_pkcs8(key_bytes.as_slice())
            .map_err(|_| "load relay Ed25519 signing key failed".to_string())?;
        let public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
        );
        Ok(Arc::new(Self {
            key_id,
            keypair,
            public_key,
        }))
    }

    pub(crate) fn key_id(&self) -> &str {
        self.key_id.as_str()
    }

    pub(crate) fn public_key(&self) -> &str {
        self.public_key.as_str()
    }

    pub(crate) fn sign_request(&self, request: &mut RelayRequest) -> Result<(), String> {
        request.platform_signature = None;
        request.platform_signature_key_id = Some(self.key_id.clone());
        request.platform_signature_alg = Some("ed25519".to_string());
        request.platform_timestamp = Some(Utc::now().timestamp());
        request.platform_nonce = Some(Uuid::new_v4().to_string());
        let payload = relay_request_signature_payload(request)?;
        request.platform_signature =
            Some(URL_SAFE_NO_PAD.encode(self.keypair.sign(payload.as_slice()).as_ref()));
        Ok(())
    }
}

pub(crate) fn validate_active_relay_signer_trust(
    signer: &PlatformRelaySigner,
    trust: &RemoteControlTrustConfigBundle,
) -> Result<(), String> {
    let Some(trusted_public_key) = trust.trusted_relay_public_keys.get(signer.key_id()) else {
        return Err(format!(
            "active relay signing key id {} is missing from trusted relay public keys",
            signer.key_id()
        ));
    };
    if trusted_public_key != signer.public_key() {
        return Err(format!(
            "active relay signing public key for key id {} does not match the trusted relay public key",
            signer.key_id()
        ));
    }
    Ok(())
}

pub(crate) fn relay_request_signature_payload(request: &RelayRequest) -> Result<Vec<u8>, String> {
    let key_id = required_text(
        request.platform_signature_key_id.as_deref(),
        "relay platform_signature_key_id",
    )?;
    let algorithm = required_text(
        request.platform_signature_alg.as_deref(),
        "relay platform_signature_alg",
    )?;
    let timestamp = request
        .platform_timestamp
        .ok_or_else(|| "relay platform_timestamp is required".to_string())?;
    let nonce = required_text(request.platform_nonce.as_deref(), "relay platform_nonce")?;
    let headers = canonical_json_string(&Value::Object(
        request
            .headers
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    ))?;
    let body = canonical_json_string(&request.body)?;
    Ok(format!(
        "v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        request.message_type,
        request.request_id,
        request.owner_user_id,
        request.device_id,
        request.workspace_id,
        request.method,
        request.path,
        key_id,
        algorithm,
        timestamp,
        nonce,
        headers,
        body,
    )
    .into_bytes())
}

fn required_text(value: Option<&str>, label: &str) -> Result<String, String> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} is required"))?;
    Ok(value.to_string())
}

pub(crate) fn canonical_json_string(value: &Value) -> Result<String, String> {
    let mut output = String::new();
    write_canonical_json(value, &mut output)?;
    Ok(output)
}

fn write_canonical_json(value: &Value, output: &mut String) -> Result<(), String> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(value.to_string().as_str()),
        Value::String(value) => {
            let encoded = serde_json::to_string(value)
                .map_err(|err| format!("encode relay signature string failed: {err}"))?;
            output.push_str(encoded.as_str());
        }
        Value::Array(values) => {
            output.push('[');
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical_json(item, output)?;
            }
            output.push(']');
        }
        Value::Object(map) => {
            output.push('{');
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                let encoded_key = serde_json::to_string(key)
                    .map_err(|err| format!("encode relay signature object key failed: {err}"))?;
                output.push_str(encoded_key.as_str());
                output.push(':');
                write_canonical_json(item, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn read_private_key_file(label: &str, path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| format!("read {label} metadata {} failed: {err}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{label} {} must be a regular non-symlink file",
            path.display()
        ));
    }
    if metadata.len() > max_bytes {
        return Err(format!("{label} {} exceeds the size limit", path.display()));
    }
    validate_private_file(path, &metadata)?;
    let content =
        fs::read(path).map_err(|err| format!("read {label} {} failed: {err}", path.display()))?;
    if content.len() as u64 > max_bytes {
        return Err(format!("{label} {} exceeds the size limit", path.display()));
    }
    Ok(content)
}

#[cfg(unix)]
fn validate_private_file(path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(format!(
            "relay signing key {} must be owned by the service user",
            path.display()
        ));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "relay signing key {} must have mode 0600 or stricter",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_file(_path: &Path, _metadata: &fs::Metadata) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
    use serde_json::json;

    use super::*;
    use crate::relay::RelayRequest;

    fn test_signer(key_id: &str) -> PlatformRelaySigner {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("generate key");
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("load keypair");
        PlatformRelaySigner {
            key_id: key_id.to_string(),
            public_key: format!(
                "ed25519:{}",
                URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
            ),
            keypair,
        }
    }

    fn trust_bundle(
        trusted_relay_public_keys: BTreeMap<String, String>,
    ) -> RemoteControlTrustConfigBundle {
        RemoteControlTrustConfigBundle {
            require_signed_messages: true,
            signature_max_skew_seconds: 120,
            trusted_relay_public_keys,
        }
    }

    #[test]
    fn canonical_json_sorts_object_keys_recursively() {
        let value = serde_json::json!({
            "z": 1,
            "a": {
                "d": [3, {"y": false, "b": true}],
                "b": "x"
            }
        });
        let canonical = canonical_json_string(&value).expect("canonical JSON");
        assert_eq!(
            canonical,
            r#"{"a":{"b":"x","d":[3,{"b":true,"y":false}]},"z":1}"#
        );
    }

    #[test]
    fn sign_request_sets_signature_fields_and_signature_verifies() {
        let signer = test_signer("relay-key-1");
        let mut request = RelayRequest {
            message_type: "plugin_execute_request".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            method: "POST".to_string(),
            path: "/plugins/execute".to_string(),
            headers: BTreeMap::from([("x-demo".to_string(), "1".to_string())]),
            body: json!({"tool":"browser","args":{"b":2,"a":1}}),
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        };

        signer.sign_request(&mut request).expect("sign request");

        assert_eq!(
            request.platform_signature_key_id.as_deref(),
            Some("relay-key-1")
        );
        assert_eq!(request.platform_signature_alg.as_deref(), Some("ed25519"));
        assert!(request.platform_timestamp.is_some());
        assert!(request.platform_nonce.is_some());

        let signature = URL_SAFE_NO_PAD
            .decode(
                request
                    .platform_signature
                    .as_deref()
                    .expect("signature should be present")
                    .as_bytes(),
            )
            .expect("decode signature");
        let payload = relay_request_signature_payload(&request).expect("signature payload");
        UnparsedPublicKey::new(&ED25519, signer.keypair.public_key().as_ref())
            .verify(payload.as_slice(), signature.as_slice())
            .expect("signature should verify");
    }

    #[test]
    fn active_relay_signer_trust_accepts_exact_key_match() {
        let signer = test_signer("relay-key-1");
        let trust = trust_bundle(BTreeMap::from([(
            signer.key_id().to_string(),
            signer.public_key().to_string(),
        )]));

        validate_active_relay_signer_trust(&signer, &trust).expect("matching trust");
    }

    #[test]
    fn active_relay_signer_trust_rejects_missing_key_id() {
        let signer = test_signer("relay-key-1");
        let trust = trust_bundle(BTreeMap::from([(
            "relay-key-2".to_string(),
            signer.public_key().to_string(),
        )]));

        let error = validate_active_relay_signer_trust(&signer, &trust)
            .expect_err("missing active key id must fail");
        assert!(error.contains("relay-key-1"));
        assert!(error.contains("missing"));
    }

    #[test]
    fn active_relay_signer_trust_rejects_public_key_mismatch() {
        let signer = test_signer("relay-key-1");
        let other_signer = test_signer("relay-key-1");
        let trust = trust_bundle(BTreeMap::from([(
            signer.key_id().to_string(),
            other_signer.public_key().to_string(),
        )]));

        let error = validate_active_relay_signer_trust(&signer, &trust)
            .expect_err("mismatched public key must fail");
        assert!(error.contains("does not match"));
    }

    #[test]
    fn active_relay_signer_trust_allows_additional_rotation_keys() {
        let signer = test_signer("relay-key-1");
        let next_signer = test_signer("relay-key-2");
        let trust = trust_bundle(BTreeMap::from([
            (signer.key_id().to_string(), signer.public_key().to_string()),
            (
                next_signer.key_id().to_string(),
                next_signer.public_key().to_string(),
            ),
        ]));

        validate_active_relay_signer_trust(&signer, &trust)
            .expect("staged rotation trust should accept the active key");
    }
}
