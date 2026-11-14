#![allow(dead_code)]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use url::Host;
use uuid::Uuid;

const MAX_IDENTITY_BYTES: usize = 256;
const MAX_HOSTS: usize = 256;
const MAX_HOST_BYTES: usize = 253;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlledNetworkPolicyEnvelope {
    pub policy_revision: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub windows_user_sid: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_ports: Vec<u16>,
    pub expires_at: String,
    pub signature_key_id: String,
    pub signature_alg: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedControlledNetworkPolicy {
    pub policy_revision: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub windows_user_sid: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_ports: Vec<u16>,
    pub expires_at: DateTime<Utc>,
}

pub(crate) fn verify_controlled_network_policy(
    policy: &ControlledNetworkPolicyEnvelope,
    expected_owner_user_id: &str,
    expected_device_id: &str,
    expected_workspace_id: &str,
    expected_windows_user_sid: &str,
    expected_key_id: &str,
    expected_public_key: &str,
    now: DateTime<Utc>,
) -> Result<VerifiedControlledNetworkPolicy, String> {
    validate_identity(&policy.policy_revision, "policy revision")?;
    Uuid::parse_str(&policy.policy_revision)
        .map_err(|_| "Controlled Network policy revision is invalid.".to_owned())?;
    validate_identity(&policy.owner_user_id, "owner user id")?;
    validate_identity(&policy.device_id, "device id")?;
    validate_identity(&policy.workspace_id, "workspace id")?;
    if policy.owner_user_id != expected_owner_user_id
        || policy.device_id != expected_device_id
        || policy.workspace_id != expected_workspace_id
    {
        return Err("Controlled Network policy identity does not match the active context.".into());
    }
    let expected_sid = normalize_windows_sid(expected_windows_user_sid)?;
    let policy_sid = normalize_windows_sid(&policy.windows_user_sid)?;
    if policy_sid != expected_sid {
        return Err(
            "Controlled Network policy Windows user SID does not match the active user.".into(),
        );
    }
    if policy.signature_alg != "ed25519" || policy.signature_key_id != expected_key_id {
        return Err("Controlled Network policy signature key is not trusted.".into());
    }
    let expires_at = DateTime::parse_from_rfc3339(&policy.expires_at)
        .map_err(|_| "Controlled Network policy expiry is invalid.".to_owned())?
        .with_timezone(&Utc);
    if expires_at <= now {
        return Err("Controlled Network policy has expired.".into());
    }
    if policy.allowed_hosts.is_empty() || policy.allowed_hosts.len() > MAX_HOSTS {
        return Err("Controlled Network policy host count is invalid.".into());
    }
    let allowed_hosts = policy
        .allowed_hosts
        .iter()
        .map(|host| normalize_host(host))
        .collect::<Result<BTreeSet<_>, _>>()?
        .into_iter()
        .collect::<Vec<_>>();
    if policy.allowed_ports.is_empty()
        || policy.allowed_ports.len() > 2
        || policy
            .allowed_ports
            .iter()
            .any(|port| !matches!(port, 80 | 443))
    {
        return Err("Controlled Network policy ports are invalid.".into());
    }
    let public_key = decode_prefixed_base64(expected_public_key, "public key", 32)?;
    let signature =
        decode_prefixed_base64(&format!("ed25519:{}", policy.signature), "signature", 64)?;
    let payload = canonical_payload(policy, &allowed_hosts);
    UnparsedPublicKey::new(&ED25519, &public_key)
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| "Controlled Network policy signature is invalid.".to_owned())?;
    Ok(VerifiedControlledNetworkPolicy {
        policy_revision: policy.policy_revision.clone(),
        owner_user_id: policy.owner_user_id.clone(),
        device_id: policy.device_id.clone(),
        workspace_id: policy.workspace_id.clone(),
        windows_user_sid: policy_sid,
        allowed_hosts,
        allowed_ports: policy
            .allowed_ports
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        expires_at,
    })
}

fn canonical_payload(policy: &ControlledNetworkPolicyEnvelope, allowed_hosts: &[String]) -> String {
    let value = serde_json::json!({
        "allowed_hosts": allowed_hosts,
        "allowed_ports": policy.allowed_ports.iter().copied().collect::<BTreeSet<_>>(),
        "device_id": policy.device_id,
        "expires_at": policy.expires_at.parse::<DateTime<Utc>>().expect("validated expiry").timestamp(),
        "owner_user_id": policy.owner_user_id,
        "policy_revision": policy.policy_revision,
        "signature_alg": policy.signature_alg,
        "signature_key_id": policy.signature_key_id,
        "windows_user_sid": policy.windows_user_sid,
        "workspace_id": policy.workspace_id,
    });
    canonical_json(&value)
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut sorted = Map::new();
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), object[key].clone());
            }
            serde_json::to_string(&sorted).expect("canonical JSON serialization")
        }
        _ => serde_json::to_string(value).expect("canonical JSON serialization"),
    }
}

fn validate_identity(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_IDENTITY_BYTES || value.chars().any(char::is_control) {
        return Err(format!("Controlled Network {label} is invalid."));
    }
    Ok(())
}

fn normalize_windows_sid(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.len() > 184
        || !value.starts_with("S-1-")
        || value
            .split('-')
            .skip(1)
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("Controlled Network Windows user SID is invalid.".into());
    }
    Ok(value.to_owned())
}

fn normalize_host(value: &str) -> Result<String, String> {
    let mut value = value.trim().trim_end_matches('.');
    let wildcard = value.starts_with("*.");
    if wildcard {
        value = &value[2..];
    }
    if value.is_empty()
        || value.contains('*')
        || value.contains(['/', ':', '@'])
        || value.chars().any(char::is_whitespace)
    {
        return Err("Controlled Network policy host is invalid.".into());
    }
    let domain =
        match Host::parse(value).map_err(|_| "Controlled Network policy host is invalid.")? {
            Host::Domain(domain) => domain.to_ascii_lowercase(),
            Host::Ipv4(_) | Host::Ipv6(_) => {
                return Err("Controlled Network policy IP literals are not allowed.".into())
            }
        };
    let labels = domain.split('.').collect::<Vec<_>>();
    if labels.len() < 2
        || domain.len() > MAX_HOST_BYTES
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err("Controlled Network policy host is invalid.".into());
    }
    Ok(if wildcard {
        format!("*.{domain}")
    } else {
        domain
    })
}

fn decode_prefixed_base64(
    value: &str,
    label: &str,
    expected_bytes: usize,
) -> Result<Vec<u8>, String> {
    let encoded = value
        .strip_prefix("ed25519:")
        .ok_or_else(|| format!("Controlled Network {label} format is invalid."))?;
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|_| format!("Controlled Network {label} is invalid."))?;
    if decoded.len() != expected_bytes {
        return Err(format!("Controlled Network {label} length is invalid."));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn signed_policy() -> (ControlledNetworkPolicyEnvelope, String) {
        let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
        let public_key = format!("ed25519:{}", URL_SAFE_NO_PAD.encode(keypair.public_key()));
        let mut policy = ControlledNetworkPolicyEnvelope {
            policy_revision: Uuid::new_v4().to_string(),
            owner_user_id: "owner-1".into(),
            device_id: "device-1".into(),
            workspace_id: "workspace-1".into(),
            windows_user_sid: "S-1-5-21-100-200-300-400".into(),
            allowed_hosts: vec!["API.Example.com.".into(), "*.example.com".into()],
            allowed_ports: vec![443, 80],
            expires_at: "2026-11-14T01:00:00Z".into(),
            signature_key_id: "network-key-1".into(),
            signature_alg: "ed25519".into(),
            signature: String::new(),
        };
        let payload =
            canonical_payload(&policy, &["*.example.com".into(), "api.example.com".into()]);
        policy.signature = URL_SAFE_NO_PAD.encode(keypair.sign(payload.as_bytes()).as_ref());
        (policy, public_key)
    }

    #[test]
    fn verifies_identity_targets_and_signature() {
        let (policy, public_key) = signed_policy();
        let verified = verify_controlled_network_policy(
            &policy,
            "owner-1",
            "device-1",
            "workspace-1",
            "S-1-5-21-100-200-300-400",
            "network-key-1",
            &public_key,
            Utc.with_ymd_and_hms(2026, 11, 13, 23, 0, 0).unwrap(),
        )
        .unwrap();
        assert_eq!(
            verified.allowed_hosts,
            vec!["*.example.com", "api.example.com"]
        );
        assert_eq!(verified.allowed_ports, vec![80, 443]);
    }

    #[test]
    fn rejects_expired_or_mismatched_policy() {
        let (mut policy, public_key) = signed_policy();
        policy.workspace_id = "other-workspace".into();
        assert!(verify_controlled_network_policy(
            &policy,
            "owner-1",
            "device-1",
            "workspace-1",
            "S-1-5-21-100-200-300-400",
            "network-key-1",
            &public_key,
            Utc.with_ymd_and_hms(2026, 11, 13, 23, 0, 0).unwrap(),
        )
        .is_err());
        let (policy, public_key) = signed_policy();
        assert!(verify_controlled_network_policy(
            &policy,
            "owner-1",
            "device-1",
            "workspace-1",
            "S-1-5-21-100-200-300-400",
            "network-key-1",
            &public_key,
            Utc.with_ymd_and_hms(2026, 11, 14, 1, 0, 0).unwrap(),
        )
        .is_err());
    }

    #[test]
    fn rejects_ip_literal_wildcard_escape_and_unknown_signature_key() {
        let (mut policy, public_key) = signed_policy();
        policy.allowed_hosts = vec!["127.0.0.1".into()];
        assert!(verify_controlled_network_policy(
            &policy,
            "owner-1",
            "device-1",
            "workspace-1",
            "S-1-5-21-100-200-300-400",
            "network-key-1",
            &public_key,
            Utc.with_ymd_and_hms(2026, 11, 13, 23, 0, 0).unwrap(),
        )
        .is_err());
        let (policy, public_key) = signed_policy();
        assert!(verify_controlled_network_policy(
            &policy,
            "owner-1",
            "device-1",
            "workspace-1",
            "S-1-5-21-100-200-300-400",
            "other-key",
            &public_key,
            Utc.with_ymd_and_hms(2026, 11, 13, 23, 0, 0).unwrap(),
        )
        .is_err());
    }
}
