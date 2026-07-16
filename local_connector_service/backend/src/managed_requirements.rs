// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chatos_sandbox_contract::{
    managed_requirements_bundle_signature_payload, parse_managed_requirements_toml,
    ManagedRequirementsBundle, ManagedRequirementsBundleLayer, ManagedRequirementsBundlePayload,
    MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
};
use chrono::{DateTime, Utc};
use ring::signature::{Ed25519KeyPair, KeyPair};
use sha2::{Digest, Sha256};
use url::Url;

use crate::config::AppConfig;

const MAX_REQUIREMENTS_BYTES: u64 = 1024 * 1024;
const MAX_REQUIREMENTS_LAYERS: usize = 64;

pub(crate) struct ManagedRequirementsSigner {
    key_id: String,
    cloud_base_url: String,
    fallback_requirements_toml: Option<String>,
    keypair: Ed25519KeyPair,
    ttl: std::time::Duration,
    public_key: String,
}

impl std::fmt::Debug for ManagedRequirementsSigner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedRequirementsSigner")
            .field("key_id", &self.key_id)
            .field("cloud_base_url", &self.cloud_base_url)
            .field("ttl", &self.ttl)
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

impl ManagedRequirementsSigner {
    pub(crate) fn load(config: &AppConfig) -> Result<Option<Arc<Self>>, String> {
        let signing_key_configured = config.managed_requirements_signing_key_path.is_some();
        let signing_key_id_configured = config.managed_requirements_signing_key_id.is_some();
        if !signing_key_configured && !signing_key_id_configured {
            if config.managed_requirements_toml_path.is_some() {
                return Err(
                    "managed requirements TOML fallback requires a signing key and signing key id"
                        .to_string(),
                );
            }
            return Ok(None);
        }
        if signing_key_configured != signing_key_id_configured {
            return Err(
                "managed requirements signing key path and signing key id must be configured together"
                    .to_string(),
            );
        }
        let cloud_base_url = config
            .public_base_url
            .as_deref()
            .ok_or_else(|| {
                "LOCAL_CONNECTOR_PUBLIC_BASE_URL is required when managed requirements signing is enabled"
                    .to_string()
            })
            .and_then(canonical_cloud_base_url)?;
        let signing_key_path = config
            .managed_requirements_signing_key_path
            .as_deref()
            .expect("validated managed requirements signing key path");
        let key_id = config
            .managed_requirements_signing_key_id
            .as_deref()
            .expect("validated managed requirements signing key id")
            .trim()
            .to_string();
        if key_id.is_empty() {
            return Err("managed requirements signing key id must not be empty".to_string());
        }
        let fallback_requirements_toml = config
            .managed_requirements_toml_path
            .as_deref()
            .map(|requirements_path| {
                let requirements_toml = read_regular_file(
                    "managed requirements TOML",
                    requirements_path,
                    MAX_REQUIREMENTS_BYTES,
                    false,
                )?;
                let requirements_toml = String::from_utf8(requirements_toml)
                    .map_err(|err| format!("managed requirements TOML must be UTF-8: {err}"))?;
                parse_managed_requirements_toml(requirements_toml.as_str())
                    .map_err(|err| format!("parse managed requirements TOML failed: {err}"))?;
                Ok::<_, String>(requirements_toml)
            })
            .transpose()?;
        let key_bytes = read_regular_file(
            "managed requirements signing key",
            signing_key_path,
            16 * 1024,
            true,
        )?;
        let keypair = Ed25519KeyPair::from_pkcs8(key_bytes.as_slice())
            .map_err(|_| "load managed requirements Ed25519 signing key failed".to_string())?;
        let public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
        );
        Ok(Some(Arc::new(Self {
            key_id,
            cloud_base_url,
            fallback_requirements_toml,
            keypair,
            ttl: config.managed_requirements_bundle_ttl,
            public_key,
        })))
    }

    pub(crate) fn public_key(&self) -> &str {
        self.public_key.as_str()
    }

    pub(crate) fn key_id(&self) -> &str {
        self.key_id.as_str()
    }

    pub(crate) fn bundle_for_device(
        &self,
        owner_user_id: &str,
        device_id: &str,
        device_public_key: &str,
        mut layers: Vec<ManagedRequirementsBundleLayer>,
        issued_at: DateTime<Utc>,
    ) -> Result<ManagedRequirementsBundle, String> {
        if layers.is_empty() {
            if let Some(fallback) = self.fallback_layer() {
                layers.push(fallback);
            }
        }
        validate_bundle_layers(layers.as_slice())?;
        let ttl = chrono::Duration::from_std(self.ttl)
            .map_err(|err| format!("managed requirements bundle TTL is invalid: {err}"))?;
        let payload = ManagedRequirementsBundlePayload {
            schema_version: MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
            key_id: self.key_id.clone(),
            cloud_base_url: self.cloud_base_url.clone(),
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            device_public_key: device_public_key.to_string(),
            issued_at: issued_at.to_rfc3339(),
            expires_at: (issued_at + ttl).to_rfc3339(),
            layers,
        };
        let signed = managed_requirements_bundle_signature_payload(&payload)
            .map_err(|err| format!("serialize managed requirements bundle failed: {err}"))?;
        let signature = URL_SAFE_NO_PAD.encode(self.keypair.sign(signed.as_slice()).as_ref());
        Ok(ManagedRequirementsBundle { payload, signature })
    }

    fn fallback_layer(&self) -> Option<ManagedRequirementsBundleLayer> {
        self.fallback_requirements_toml
            .as_ref()
            .map(|requirements_toml| ManagedRequirementsBundleLayer {
                policy_id: "static-environment-policy".to_string(),
                policy_version: 1,
                assignment_id: "static-environment-fallback".to_string(),
                assignment_scope: "service_fallback".to_string(),
                requirements_sha256: requirements_digest(requirements_toml.as_bytes()),
                requirements_toml: requirements_toml.clone(),
            })
    }
}

fn validate_bundle_layers(layers: &[ManagedRequirementsBundleLayer]) -> Result<(), String> {
    if layers.len() > MAX_REQUIREMENTS_LAYERS {
        return Err("managed requirements bundle contains too many policy layers".to_string());
    }
    let mut aggregate_bytes = 0_u64;
    let mut assignment_ids = HashSet::new();
    for layer in layers {
        validate_layer_metadata(layer.policy_id.as_str(), "policy_id")?;
        validate_layer_metadata(layer.assignment_id.as_str(), "assignment_id")?;
        validate_layer_metadata(layer.assignment_scope.as_str(), "assignment_scope")?;
        if layer.policy_version < 1 {
            return Err("managed requirements bundle policy_version must be positive".to_string());
        }
        if !assignment_ids.insert(layer.assignment_id.as_str()) {
            return Err(
                "managed requirements bundle contains duplicate assignment ids".to_string(),
            );
        }
        aggregate_bytes = aggregate_bytes
            .checked_add(layer.requirements_toml.len() as u64)
            .ok_or_else(|| "managed requirements bundle size overflow".to_string())?;
        if aggregate_bytes > MAX_REQUIREMENTS_BYTES {
            return Err(
                "managed requirements bundle TOML exceeds the 1 MiB aggregate limit".to_string(),
            );
        }
        if layer.requirements_sha256 != requirements_digest(layer.requirements_toml.as_bytes()) {
            return Err("managed requirements bundle content digest does not match".to_string());
        }
        parse_managed_requirements_toml(layer.requirements_toml.as_str())
            .map_err(|err| format!("parse managed requirements bundle TOML failed: {err}"))?;
    }
    Ok(())
}

fn validate_layer_metadata(value: &str, label: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 {
        Err(format!(
            "managed requirements bundle layer {label} is invalid"
        ))
    } else {
        Ok(())
    }
}

fn read_regular_file(
    label: &str,
    path: &Path,
    max_bytes: u64,
    private: bool,
) -> Result<Vec<u8>, String> {
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
    if private {
        validate_private_file(path, &metadata)?;
    } else {
        validate_policy_file(path, &metadata)?;
    }
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
            "managed requirements signing key {} must be owned by the service user",
            path.display()
        ));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "managed requirements signing key {} must have mode 0600 or stricter",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_policy_file(path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let owner = metadata.uid();
    let service_user = unsafe { libc::geteuid() };
    if owner != 0 && owner != service_user {
        return Err(format!(
            "managed requirements TOML {} must be owned by root or the service user",
            path.display()
        ));
    }
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(format!(
            "managed requirements TOML {} must not be group- or world-writable",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_file(_path: &Path, _metadata: &fs::Metadata) -> Result<(), String> {
    Ok(())
}

#[cfg(not(unix))]
fn validate_policy_file(_path: &Path, _metadata: &fs::Metadata) -> Result<(), String> {
    Ok(())
}

fn canonical_cloud_base_url(value: &str) -> Result<String, String> {
    let mut url = Url::parse(value.trim())
        .map_err(|err| format!("LOCAL_CONNECTOR_PUBLIC_BASE_URL is invalid: {err}"))?;
    if url.cannot_be_a_base()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "LOCAL_CONNECTOR_PUBLIC_BASE_URL must not contain credentials, query, or fragment"
                .to_string(),
        );
    }
    let normalized_path = url.path().trim_end_matches('/').to_string();
    url.set_path(if normalized_path.is_empty() {
        "/"
    } else {
        normalized_path.as_str()
    });
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn requirements_digest(value: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value)))
}

#[cfg(test)]
mod tests {
    use ring::rand::SystemRandom;
    use ring::signature::{UnparsedPublicKey, ED25519};

    use super::*;

    fn test_signer(
        fallback_requirements_toml: Option<&str>,
    ) -> (ManagedRequirementsSigner, Vec<u8>) {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let public_key = keypair.public_key().as_ref().to_vec();
        (
            ManagedRequirementsSigner {
                key_id: "key-1".to_string(),
                cloud_base_url: "https://connector.example.test".to_string(),
                fallback_requirements_toml: fallback_requirements_toml.map(str::to_string),
                keypair,
                ttl: std::time::Duration::from_secs(3600),
                public_key: format!("ed25519:{}", URL_SAFE_NO_PAD.encode(public_key.as_slice())),
            },
            public_key,
        )
    }

    #[test]
    fn bundle_signature_binds_device_identity_and_policy() {
        let (signer, public_key) = test_signer(Some("default_permissions = \":read-only\""));

        let bundle = signer
            .bundle_for_device(
                "user-1",
                "device-1",
                "ed25519:device",
                Vec::new(),
                Utc::now(),
            )
            .unwrap();
        assert_eq!(bundle.payload.layers.len(), 1);
        let signed = managed_requirements_bundle_signature_payload(&bundle.payload).unwrap();
        let signature = URL_SAFE_NO_PAD.decode(bundle.signature.as_bytes()).unwrap();

        UnparsedPublicKey::new(&ED25519, public_key)
            .verify(signed.as_slice(), signature.as_slice())
            .expect("service signature should verify");
    }

    #[test]
    fn database_layers_suppress_the_static_environment_fallback() {
        let (signer, _) = test_signer(Some("default_permissions = \":read-only\""));
        let requirements_toml = "default_permissions = \":workspace\"";
        let database_layer = ManagedRequirementsBundleLayer {
            policy_id: "policy-db".to_string(),
            policy_version: 4,
            assignment_id: "assignment-db".to_string(),
            assignment_scope: "user".to_string(),
            requirements_toml: requirements_toml.to_string(),
            requirements_sha256: requirements_digest(requirements_toml.as_bytes()),
        };

        let bundle = signer
            .bundle_for_device(
                "user-1",
                "device-1",
                "ed25519:device",
                vec![database_layer],
                Utc::now(),
            )
            .unwrap();

        assert_eq!(bundle.payload.layers.len(), 1);
        assert_eq!(bundle.payload.layers[0].assignment_id, "assignment-db");
    }

    #[test]
    fn signer_can_issue_an_explicit_empty_layers_bundle_without_a_fallback() {
        let (signer, _) = test_signer(None);

        let bundle = signer
            .bundle_for_device(
                "user-1",
                "device-1",
                "ed25519:device",
                Vec::new(),
                Utc::now(),
            )
            .unwrap();

        assert!(bundle.payload.layers.is_empty());
    }

    #[test]
    fn signer_rejects_layers_that_clients_would_reject() {
        let (signer, _) = test_signer(None);
        let mut layer = ManagedRequirementsBundleLayer {
            policy_id: "policy-db".to_string(),
            policy_version: 1,
            assignment_id: "assignment-db".to_string(),
            assignment_scope: "global".to_string(),
            requirements_toml: "default_permissions = \":read-only\"".to_string(),
            requirements_sha256: "sha256:stale".to_string(),
        };

        let error = signer
            .bundle_for_device(
                "user-1",
                "device-1",
                "ed25519:device",
                vec![layer.clone()],
                Utc::now(),
            )
            .expect_err("stale database digest must not be signed");
        assert!(error.contains("digest"));

        layer.requirements_sha256 = requirements_digest(layer.requirements_toml.as_bytes());
        let too_many = (0..=MAX_REQUIREMENTS_LAYERS)
            .map(|index| ManagedRequirementsBundleLayer {
                policy_id: format!("policy-{index}"),
                assignment_id: format!("assignment-{index}"),
                ..layer.clone()
            })
            .collect();
        let error = signer
            .bundle_for_device("user-1", "device-1", "ed25519:device", too_many, Utc::now())
            .expect_err("too many layers must not be signed");
        assert!(error.contains("too many"));
    }
}
