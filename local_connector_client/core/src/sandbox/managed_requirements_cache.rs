// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::ManagedRequirementsBundle;
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::device_keys::{sign_device_message, verify_device_message_signature};
use crate::LocalState;

const CACHE_SCHEMA_VERSION: u32 = 1;
const CACHE_FILE_NAME: &str = "managed-requirements-cache.json";
const CACHE_SIGNATURE_DOMAIN: &[u8] = b"chatos-managed-requirements-cache-v1\0";
const MAX_CACHE_BYTES: u64 = 1024 * 1024 + 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedRequirementsIdentity {
    pub(crate) cloud_base_url: String,
    pub(crate) owner_user_id: String,
    pub(crate) device_id: String,
    pub(crate) device_public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedRequirementsCacheEntry {
    schema_version: u32,
    bundle: ManagedRequirementsBundle,
    device_cache_signature: String,
}

impl ManagedRequirementsIdentity {
    pub(crate) fn from_state(state: &LocalState) -> Result<Option<Self>> {
        let cloud_base_url = state
            .paired_cloud_base_url
            .as_deref()
            .or_else(|| state.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()));
        let owner_user_id = state.paired_user_id.as_deref().or_else(|| {
            state
                .auth
                .as_ref()
                .and_then(|auth| auth.user.as_ref().map(|user| user.id.as_str()))
        });
        let Some((cloud_base_url, owner_user_id, device_id, device_public_key)) = cloud_base_url
            .zip(owner_user_id)
            .zip(state.device_id.as_deref())
            .zip(state.device_public_key.as_deref())
            .map(
                |(((cloud_base_url, owner_user_id), device_id), device_public_key)| {
                    (cloud_base_url, owner_user_id, device_id, device_public_key)
                },
            )
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            cloud_base_url: canonical_cloud_base_url(cloud_base_url)?,
            owner_user_id: required_identity_value(owner_user_id, "paired user id")?,
            device_id: required_identity_value(device_id, "device id")?,
            device_public_key: required_identity_value(device_public_key, "device public key")?,
        }))
    }
}

pub(crate) fn load_cached_bundle(
    state_path: &Path,
    identity: &ManagedRequirementsIdentity,
) -> Result<Option<ManagedRequirementsBundle>> {
    let path = cache_path(state_path);
    let metadata = match fs::symlink_metadata(path.as_path()) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("read managed requirements cache {}", path.display()))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!(
            "managed requirements cache {} must be a regular non-symlink file",
            path.display()
        ));
    }
    validate_cache_file_permissions(path.as_path(), &metadata)?;
    if metadata.len() > MAX_CACHE_BYTES {
        return Err(anyhow!(
            "managed requirements cache {} exceeds the size limit",
            path.display()
        ));
    }
    let bytes = fs::read(path.as_path())
        .with_context(|| format!("read managed requirements cache {}", path.display()))?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(anyhow!(
            "managed requirements cache {} exceeds the size limit",
            path.display()
        ));
    }
    let entry = serde_json::from_slice::<ManagedRequirementsCacheEntry>(bytes.as_slice())
        .with_context(|| format!("parse managed requirements cache {}", path.display()))?;
    validate_cache_entry(&entry, identity)
        .with_context(|| format!("validate managed requirements cache {}", path.display()))?;
    Ok(Some(entry.bundle))
}

pub(crate) fn store_verified_bundle(
    state_path: &Path,
    identity: &ManagedRequirementsIdentity,
    bundle: &ManagedRequirementsBundle,
) -> Result<()> {
    ensure_bundle_identity(bundle, identity)?;
    let signed_payload = cache_signature_payload(bundle)?;
    let device_cache_signature = sign_device_message(
        state_path,
        identity.device_public_key.as_str(),
        signed_payload.as_slice(),
    )?;
    write_cache_entry(
        state_path,
        &ManagedRequirementsCacheEntry {
            schema_version: CACHE_SCHEMA_VERSION,
            bundle: bundle.clone(),
            device_cache_signature,
        },
    )
}

#[cfg(test)]
pub(super) fn store_test_bundle_with_signer(
    state_path: &Path,
    identity: &ManagedRequirementsIdentity,
    bundle: &ManagedRequirementsBundle,
    signer: impl FnOnce(&[u8]) -> String,
) -> Result<()> {
    ensure_bundle_identity(bundle, identity)?;
    let signed_payload = cache_signature_payload(bundle)?;
    write_cache_entry(
        state_path,
        &ManagedRequirementsCacheEntry {
            schema_version: CACHE_SCHEMA_VERSION,
            bundle: bundle.clone(),
            device_cache_signature: signer(signed_payload.as_slice()),
        },
    )
}

fn validate_cache_entry(
    entry: &ManagedRequirementsCacheEntry,
    identity: &ManagedRequirementsIdentity,
) -> Result<()> {
    if entry.schema_version != CACHE_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported managed requirements cache schema version {}",
            entry.schema_version
        ));
    }
    ensure_bundle_identity(&entry.bundle, identity)?;
    let signed_payload = cache_signature_payload(&entry.bundle)?;
    verify_device_message_signature(
        identity.device_public_key.as_str(),
        signed_payload.as_slice(),
        entry.device_cache_signature.as_str(),
    )
    .context("verify managed requirements cache device signature")
}

fn ensure_bundle_identity(
    bundle: &ManagedRequirementsBundle,
    identity: &ManagedRequirementsIdentity,
) -> Result<()> {
    let payload = &bundle.payload;
    ensure_identity_field(
        "cloud base URL",
        payload.cloud_base_url.as_str(),
        identity.cloud_base_url.as_str(),
    )?;
    ensure_identity_field(
        "owner user id",
        payload.owner_user_id.as_str(),
        identity.owner_user_id.as_str(),
    )?;
    ensure_identity_field(
        "device id",
        payload.device_id.as_str(),
        identity.device_id.as_str(),
    )?;
    ensure_identity_field(
        "device public key",
        payload.device_public_key.as_str(),
        identity.device_public_key.as_str(),
    )
}

fn cache_signature_payload(bundle: &ManagedRequirementsBundle) -> Result<Vec<u8>> {
    let encoded = serde_json::to_vec(bundle).context("serialize managed requirements cache")?;
    let mut signed = Vec::with_capacity(CACHE_SIGNATURE_DOMAIN.len() + encoded.len());
    signed.extend_from_slice(CACHE_SIGNATURE_DOMAIN);
    signed.extend_from_slice(encoded.as_slice());
    Ok(signed)
}

fn write_cache_entry(state_path: &Path, entry: &ManagedRequirementsCacheEntry) -> Result<()> {
    let path = cache_path(state_path);
    if let Ok(metadata) = fs::symlink_metadata(path.as_path()) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "managed requirements cache {} must be a regular non-symlink file",
                path.display()
            ));
        }
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create managed requirements cache dir {}", parent.display()))?;
    let encoded =
        serde_json::to_vec_pretty(entry).context("serialize managed requirements cache")?;
    if encoded.len() as u64 > MAX_CACHE_BYTES {
        return Err(anyhow!("managed requirements cache exceeds the size limit"));
    }
    let temp_path = parent.join(format!(".{CACHE_FILE_NAME}.{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(temp_path.as_path()).with_context(|| {
            format!("create managed requirements cache {}", temp_path.display())
        })?;
        file.write_all(encoded.as_slice())
            .with_context(|| format!("write managed requirements cache {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("sync managed requirements cache {}", temp_path.display()))?;
        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(path.as_path()).with_context(|| {
                format!("replace managed requirements cache {}", path.display())
            })?;
        }
        fs::rename(temp_path.as_path(), path.as_path()).with_context(|| {
            format!(
                "install managed requirements cache {} from {}",
                path.display(),
                temp_path.display()
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path.as_path(), fs::Permissions::from_mode(0o600)).with_context(
                || format!("restrict managed requirements cache {}", path.display()),
            )?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
}

fn cache_path(state_path: &Path) -> PathBuf {
    state_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(CACHE_FILE_NAME)
}

pub(crate) fn canonical_cloud_base_url(value: &str) -> Result<String> {
    let mut url = Url::parse(value.trim()).context("paired cloud base URL must be a valid URL")?;
    if url.cannot_be_a_base()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(anyhow!(
            "paired cloud base URL must not contain credentials, query, or fragment"
        ));
    }
    let normalized_path = url.path().trim_end_matches('/').to_string();
    url.set_path(if normalized_path.is_empty() {
        "/"
    } else {
        normalized_path.as_str()
    });
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn required_identity_value(value: &str, label: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(anyhow!("managed requirements cache {label} is empty"))
    } else {
        Ok(value.to_string())
    }
}

fn ensure_identity_field(label: &str, cached: &str, expected: &str) -> Result<()> {
    if cached == expected {
        Ok(())
    } else {
        Err(anyhow!(
            "managed requirements cache {label} does not match the current pairing"
        ))
    }
}

#[cfg(unix)]
fn validate_cache_file_permissions(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(anyhow!(
            "managed requirements cache {} must be owned by the current user",
            path.display()
        ));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(anyhow!(
            "managed requirements cache {} must not be accessible by group or other users",
            path.display()
        ));
    }
    if metadata.nlink() != 1 {
        return Err(anyhow!(
            "managed requirements cache {} must not have hard links",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_cache_file_permissions(_path: &Path, _metadata: &fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use chatos_sandbox_contract::{
        ManagedRequirementsBundleLayer, ManagedRequirementsBundlePayload,
        MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
    };
    use chrono::{Duration, Utc};
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use super::*;

    struct TestIdentity {
        identity: ManagedRequirementsIdentity,
        keypair: Ed25519KeyPair,
    }

    impl TestIdentity {
        fn new() -> Self {
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .expect("generate test keypair");
            let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parse test keypair");
            let public_key = format!(
                "ed25519:{}",
                URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
            );
            Self {
                identity: ManagedRequirementsIdentity {
                    cloud_base_url: "https://connector.example.test".to_string(),
                    owner_user_id: "user-1".to_string(),
                    device_id: "device-1".to_string(),
                    device_public_key: public_key,
                },
                keypair,
            }
        }

        fn bundle(&self) -> ManagedRequirementsBundle {
            let now = Utc::now();
            ManagedRequirementsBundle {
                payload: ManagedRequirementsBundlePayload {
                    schema_version: MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
                    key_id: "service-key-1".to_string(),
                    cloud_base_url: self.identity.cloud_base_url.clone(),
                    owner_user_id: self.identity.owner_user_id.clone(),
                    device_id: self.identity.device_id.clone(),
                    device_public_key: self.identity.device_public_key.clone(),
                    issued_at: now.to_rfc3339(),
                    expires_at: (now + Duration::hours(1)).to_rfc3339(),
                    layers: vec![ManagedRequirementsBundleLayer {
                        policy_id: "policy-1".to_string(),
                        policy_version: 1,
                        assignment_id: "assignment-1".to_string(),
                        assignment_scope: "global".to_string(),
                        requirements_toml: "default_permissions = \":read-only\"".to_string(),
                        requirements_sha256: "sha256:test".to_string(),
                    }],
                },
                signature: "service-signature".to_string(),
            }
        }

        fn entry(&self, bundle: ManagedRequirementsBundle) -> ManagedRequirementsCacheEntry {
            let signed = cache_signature_payload(&bundle).unwrap();
            ManagedRequirementsCacheEntry {
                schema_version: CACHE_SCHEMA_VERSION,
                bundle,
                device_cache_signature: URL_SAFE_NO_PAD
                    .encode(self.keypair.sign(signed.as_slice()).as_ref()),
            }
        }
    }

    fn test_state_path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("chatos-managed-cache-{label}-{}", Uuid::new_v4()))
            .join("state.json")
    }

    #[test]
    fn valid_device_signed_cache_loads_for_exact_identity() {
        let test = TestIdentity::new();
        let state_path = test_state_path("valid");
        let bundle = test.bundle();
        write_cache_entry(state_path.as_path(), &test.entry(bundle.clone())).unwrap();

        assert_eq!(
            load_cached_bundle(state_path.as_path(), &test.identity).unwrap(),
            Some(bundle)
        );
        let _ = fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn bundle_or_device_signature_tampering_is_rejected() {
        let test = TestIdentity::new();
        let state_path = test_state_path("tamper");
        let mut entry = test.entry(test.bundle());
        entry.bundle.payload.layers[0].requirements_toml = "tampered".to_string();
        write_cache_entry(state_path.as_path(), &entry).unwrap();

        let error = load_cached_bundle(state_path.as_path(), &test.identity)
            .expect_err("tampered bundle must fail");

        assert!(format!("{error:#}").contains("device signature"));
        let _ = fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn pairing_or_device_key_change_is_rejected() {
        let test = TestIdentity::new();
        for (label, mutate) in [
            ("user", 0_u8),
            ("device", 1_u8),
            ("cloud", 2_u8),
            ("key", 3_u8),
        ] {
            let state_path = test_state_path(label);
            write_cache_entry(state_path.as_path(), &test.entry(test.bundle())).unwrap();
            let mut identity = test.identity.clone();
            match mutate {
                0 => identity.owner_user_id = "user-2".to_string(),
                1 => identity.device_id = "device-2".to_string(),
                2 => identity.cloud_base_url = "https://other.example.test".to_string(),
                3 => identity.device_public_key = TestIdentity::new().identity.device_public_key,
                _ => unreachable!(),
            }

            let error = load_cached_bundle(state_path.as_path(), &identity)
                .expect_err("identity mismatch must fail");
            assert!(format!("{error:#}").contains("current pairing"));
            let _ = fs::remove_dir_all(state_path.parent().unwrap());
        }
    }

    #[cfg(unix)]
    #[test]
    fn cache_file_must_be_private() {
        use std::os::unix::fs::PermissionsExt;

        let test = TestIdentity::new();
        let state_path = test_state_path("permissions");
        write_cache_entry(state_path.as_path(), &test.entry(test.bundle())).unwrap();
        let path = cache_path(state_path.as_path());
        fs::set_permissions(path.as_path(), fs::Permissions::from_mode(0o640)).unwrap();

        let error = load_cached_bundle(state_path.as_path(), &test.identity)
            .expect_err("shared cache must fail");

        assert!(format!("{error:#}").contains("group or other users"));
        let _ = fs::remove_dir_all(state_path.parent().unwrap());
    }
}
