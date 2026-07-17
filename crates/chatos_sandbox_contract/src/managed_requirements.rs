// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

pub const MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION: u32 = 2;
pub const MANAGED_REQUIREMENTS_BUNDLE_SIGNATURE_DOMAIN: &[u8] =
    b"chatos-managed-requirements-bundle-v2\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRequirementsBundle {
    pub payload: ManagedRequirementsBundlePayload,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRequirementsBundlePayload {
    pub schema_version: u32,
    pub key_id: String,
    pub cloud_base_url: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub device_public_key: String,
    pub issued_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub layers: Vec<ManagedRequirementsBundleLayer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRequirementsBundleLayer {
    pub policy_id: String,
    pub policy_version: i64,
    pub assignment_id: String,
    pub assignment_scope: String,
    pub requirements_toml: String,
    pub requirements_sha256: String,
}

pub fn managed_requirements_bundle_signature_payload(
    payload: &ManagedRequirementsBundlePayload,
) -> Result<Vec<u8>, serde_json::Error> {
    let encoded = serde_json::to_vec(payload)?;
    let mut signed =
        Vec::with_capacity(MANAGED_REQUIREMENTS_BUNDLE_SIGNATURE_DOMAIN.len() + encoded.len());
    signed.extend_from_slice(MANAGED_REQUIREMENTS_BUNDLE_SIGNATURE_DOMAIN);
    signed.extend_from_slice(encoded.as_slice());
    Ok(signed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_payload_is_domain_separated_and_deterministic() {
        let payload = ManagedRequirementsBundlePayload {
            schema_version: MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
            key_id: "key-1".to_string(),
            cloud_base_url: "https://connector.example.test".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            device_public_key: "ed25519:test".to_string(),
            issued_at: "2026-07-15T00:00:00Z".to_string(),
            expires_at: "2026-07-16T00:00:00Z".to_string(),
            layers: vec![ManagedRequirementsBundleLayer {
                policy_id: "policy-1".to_string(),
                policy_version: 3,
                assignment_id: "assignment-1".to_string(),
                assignment_scope: "user".to_string(),
                requirements_toml: "default_permissions = \":read-only\"".to_string(),
                requirements_sha256: "sha256:test".to_string(),
            }],
        };

        let first = managed_requirements_bundle_signature_payload(&payload).unwrap();
        let second = managed_requirements_bundle_signature_payload(&payload).unwrap();

        assert_eq!(first, second);
        assert!(first.starts_with(MANAGED_REQUIREMENTS_BUNDLE_SIGNATURE_DOMAIN));
    }

    #[test]
    fn layer_order_and_schema_are_part_of_the_signed_payload() {
        let layer = |id: &str| ManagedRequirementsBundleLayer {
            policy_id: format!("policy-{id}"),
            policy_version: 1,
            assignment_id: format!("assignment-{id}"),
            assignment_scope: id.to_string(),
            requirements_toml: String::new(),
            requirements_sha256: "sha256:empty".to_string(),
        };
        let mut payload = ManagedRequirementsBundlePayload {
            schema_version: MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
            key_id: "key-1".to_string(),
            cloud_base_url: "https://connector.example.test".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            device_public_key: "ed25519:test".to_string(),
            issued_at: "2026-07-15T00:00:00Z".to_string(),
            expires_at: "2026-07-16T00:00:00Z".to_string(),
            layers: vec![layer("global"), layer("user")],
        };
        let ordered = managed_requirements_bundle_signature_payload(&payload).unwrap();
        payload.layers.reverse();
        let reversed = managed_requirements_bundle_signature_payload(&payload).unwrap();

        assert_eq!(payload.schema_version, 2);
        assert_ne!(ordered, reversed);
    }
}
