// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, SecondsFormat, Utc};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Host;
use uuid::Uuid;

use chatos_sandbox_contract::{
    CodexPermissionProfileDocument, NetworkDomainPermission, NetworkPermissionPolicy,
    PermissionProfileProvenance,
};

use crate::config::AppConfig;
use crate::relay_signature::canonical_json_string;

const MAX_HOSTS: usize = 256;
const MAX_KEY_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlledNetworkPolicyRequest {
    #[serde(default)]
    pub permission_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
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

pub(crate) struct ControlledNetworkPolicySigner {
    key_id: String,
    keypair: Ed25519KeyPair,
    public_key: String,
    ttl: std::time::Duration,
}

impl std::fmt::Debug for ControlledNetworkPolicySigner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControlledNetworkPolicySigner")
            .field("key_id", &self.key_id)
            .field("public_key", &self.public_key)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl ControlledNetworkPolicySigner {
    pub(crate) fn load(config: &AppConfig) -> Result<Option<Arc<Self>>, String> {
        let path = config.controlled_network_signing_key_path.as_deref();
        let key_id = config.controlled_network_signing_key_id.as_deref();
        match (path, key_id) {
            (None, None) => return Ok(None),
            (Some(_), None) | (None, Some(_)) => {
                return Err(
                    "controlled-network signing key path and key id must be configured together"
                        .to_string(),
                )
            }
            (Some(_), Some(_)) => {}
        }
        let path = path.expect("checked controlled-network key path");
        let metadata = fs::symlink_metadata(path)
            .map_err(|err| format!("read controlled-network signing key metadata failed: {err}"))?;
        if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_KEY_BYTES
        {
            return Err("controlled-network signing key must be a small regular file".to_string());
        }
        let key_bytes = fs::read(path)
            .map_err(|err| format!("read controlled-network signing key failed: {err}"))?;
        let keypair = Ed25519KeyPair::from_pkcs8(key_bytes.as_slice())
            .map_err(|_| "load controlled-network Ed25519 signing key failed".to_string())?;
        let key_id = key_id
            .expect("checked controlled-network key id")
            .trim()
            .to_string();
        if key_id.is_empty() || key_id.len() > 128 {
            return Err("controlled-network signing key id is invalid".to_string());
        }
        let public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
        );
        Ok(Some(Arc::new(Self {
            key_id,
            keypair,
            public_key,
            ttl: config.controlled_network_policy_ttl,
        })))
    }

    pub(crate) fn key_id(&self) -> &str {
        self.key_id.as_str()
    }

    pub(crate) fn public_key(&self) -> &str {
        self.public_key.as_str()
    }

    pub(crate) fn issue(
        &self,
        owner_user_id: &str,
        device_id: &str,
        workspace_id: &str,
        windows_user_sid: &str,
        allowed_hosts: Vec<String>,
        allowed_ports: Vec<u16>,
        now: DateTime<Utc>,
    ) -> Result<ControlledNetworkPolicyEnvelope, String> {
        validate_identity(owner_user_id, "owner user id")?;
        validate_identity(device_id, "device id")?;
        validate_identity(workspace_id, "workspace id")?;
        let windows_user_sid = normalize_windows_sid(windows_user_sid)?;
        if allowed_hosts.is_empty() || allowed_hosts.len() > MAX_HOSTS {
            return Err("controlled-network host count is invalid".to_string());
        }
        let allowed_hosts = allowed_hosts
            .iter()
            .map(|host| normalize_host(host))
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect::<Vec<_>>();
        let allowed_ports = if allowed_ports.is_empty() {
            vec![80, 443]
        } else {
            allowed_ports
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        };
        if allowed_ports.is_empty()
            || allowed_ports.len() > 2
            || allowed_ports.iter().any(|port| !matches!(port, 80 | 443))
        {
            return Err("controlled-network only supports HTTP and HTTPS ports".to_string());
        }
        let ttl = chrono::Duration::from_std(self.ttl)
            .map_err(|err| format!("controlled-network policy TTL is invalid: {err}"))?;
        let expires_at = now + ttl;
        let policy_revision = Uuid::new_v4().to_string();
        let payload = json!({
            "allowed_hosts": allowed_hosts,
            "allowed_ports": allowed_ports,
            "device_id": device_id,
            "expires_at": expires_at.timestamp(),
            "owner_user_id": owner_user_id,
            "policy_revision": policy_revision,
            "signature_alg": "ed25519",
            "signature_key_id": self.key_id,
            "windows_user_sid": windows_user_sid,
            "workspace_id": workspace_id,
        });
        let canonical = canonical_json_string(&payload)?;
        let signature = URL_SAFE_NO_PAD.encode(self.keypair.sign(canonical.as_bytes()).as_ref());
        Ok(ControlledNetworkPolicyEnvelope {
            policy_revision,
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            workspace_id: workspace_id.to_string(),
            windows_user_sid,
            allowed_hosts,
            allowed_ports,
            expires_at: expires_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            signature_key_id: self.key_id.clone(),
            signature_alg: "ed25519".to_string(),
            signature,
        })
    }
}

fn validate_identity(value: &str, label: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(format!("controlled-network {label} is invalid"))
    } else {
        Ok(())
    }
}

pub(crate) fn normalize_windows_sid(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.len() > 184
        || !value.starts_with("S-1-")
        || value
            .split('-')
            .skip(1)
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("controlled-network Windows user SID is invalid".to_string());
    }
    Ok(value.to_string())
}

pub(crate) fn allowed_hosts_from_managed_requirements(
    document: &CodexPermissionProfileDocument,
    request: &ControlledNetworkPolicyRequest,
) -> Result<Option<Vec<String>>, String> {
    let Some(profile) = request
        .permission_profile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(document.default_permissions.as_deref())
    else {
        return Ok(None);
    };
    if !document.configuration.profile_allowed(profile) {
        return Err(format!(
            "controlled-network permission profile {profile:?} is not allowed"
        ));
    }
    let resolved = document.configuration.resolve(
        profile,
        Vec::new(),
        None,
        PermissionProfileProvenance::Managed,
    )?;
    let NetworkPermissionPolicy::Restricted { requirements } =
        resolved.effective_permissions.network
    else {
        return Ok(None);
    };
    if requirements.enabled != Some(true) {
        return Ok(None);
    }
    if requirements.domains.as_ref().is_some_and(|domains| {
        domains
            .values()
            .any(|permission| *permission == NetworkDomainPermission::Deny)
    }) || requirements
        .denied_domains
        .as_ref()
        .is_some_and(|domains| !domains.is_empty())
    {
        return Err(
            "controlled-network cannot safely compile domain policies containing deny exceptions"
                .to_string(),
        );
    }
    let mut hosts = requirements
        .domains
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(host, permission)| {
            (permission == NetworkDomainPermission::Allow).then_some(host)
        })
        .collect::<Vec<_>>();
    hosts.extend(requirements.allowed_domains.unwrap_or_default());
    if hosts.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        hosts
            .iter()
            .map(|host| normalize_host(host))
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect(),
    ))
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
        return Err("controlled-network host is invalid".to_string());
    }
    let domain = match Host::parse(value).map_err(|_| "controlled-network host is invalid")? {
        Host::Domain(domain) => domain.to_ascii_lowercase(),
        Host::Ipv4(_) | Host::Ipv6(_) => {
            return Err("controlled-network IP literals are not allowed".to_string())
        }
    };
    let labels = domain.split('.').collect::<Vec<_>>();
    if labels.len() < 2
        || domain.len() > 253
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
        return Err("controlled-network host is invalid".to_string());
    }
    Ok(if wildcard {
        format!("*.{domain}")
    } else {
        domain
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{UnparsedPublicKey, ED25519};

    fn signer() -> ControlledNetworkPolicySigner {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("generate key");
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("load key");
        let public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
        );
        ControlledNetworkPolicySigner {
            key_id: "network-key-1".to_string(),
            keypair,
            public_key,
            ttl: std::time::Duration::from_secs(300),
        }
    }

    #[test]
    fn issues_canonical_policy_with_normalized_hosts_and_valid_signature() {
        let signer = signer();
        let now = DateTime::parse_from_rfc3339("2026-08-31T00:00:00Z")
            .expect("time")
            .with_timezone(&Utc);
        let policy = signer
            .issue(
                "owner-1",
                "device-1",
                "workspace-1",
                "S-1-5-21-100-200-300-400",
                vec![
                    "API.Example.com.".to_string(),
                    "*.Example.com".to_string(),
                    "例子.测试".to_string(),
                ],
                vec![443, 80, 443],
                now,
            )
            .expect("policy");

        assert_eq!(
            policy.allowed_hosts,
            vec![
                "*.example.com",
                "api.example.com",
                "xn--fsqu00a.xn--0zwm56d"
            ]
        );
        assert_eq!(policy.allowed_ports, vec![80, 443]);
        let expires_at = DateTime::parse_from_rfc3339(policy.expires_at.as_str())
            .expect("expiry")
            .timestamp();
        let payload = json!({
            "allowed_hosts": policy.allowed_hosts,
            "allowed_ports": policy.allowed_ports,
            "device_id": policy.device_id,
            "expires_at": expires_at,
            "owner_user_id": policy.owner_user_id,
            "policy_revision": policy.policy_revision,
            "signature_alg": policy.signature_alg,
            "signature_key_id": policy.signature_key_id,
            "windows_user_sid": policy.windows_user_sid,
            "workspace_id": policy.workspace_id,
        });
        let canonical = canonical_json_string(&payload).expect("canonical payload");
        let signature = URL_SAFE_NO_PAD
            .decode(policy.signature.as_bytes())
            .expect("signature");
        UnparsedPublicKey::new(&ED25519, signer.keypair.public_key().as_ref())
            .verify(canonical.as_bytes(), signature.as_slice())
            .expect("signature verifies");
    }

    #[test]
    fn rejects_ip_literals_invalid_wildcards_and_unsupported_ports() {
        for host in ["127.0.0.1", "[::1]", "*.*.example.com", "localhost"] {
            assert!(normalize_host(host).is_err(), "host should fail: {host}");
        }
        let signer = signer();
        let result = signer.issue(
            "owner-1",
            "device-1",
            "workspace-1",
            "S-1-5-21-100-200-300-400",
            vec!["example.com".to_string()],
            vec![22],
            Utc::now(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn derives_hosts_only_from_managed_allowed_domains() {
        let document = chatos_sandbox_contract::parse_managed_requirements_toml(
            r#"
default_permissions = "windows-controlled"

[allowed_permission_profiles]
"windows-controlled" = true

[permissions.windows-controlled]
extends = ":workspace"

[permissions.windows-controlled.network]
enabled = true
mode = "full"

[permissions.windows-controlled.network.domains]
"API.Example.com" = "allow"
"*.Example.org" = "allow"
"#,
        )
        .expect("managed requirements");
        assert_eq!(
            allowed_hosts_from_managed_requirements(
                &document,
                &ControlledNetworkPolicyRequest::default(),
            )
            .expect("hosts"),
            Some(vec![
                "*.example.org".to_string(),
                "api.example.com".to_string(),
            ])
        );
    }

    #[test]
    fn refuses_deny_exceptions_that_wfp_allow_rules_cannot_express() {
        let document = chatos_sandbox_contract::parse_managed_requirements_toml(
            r#"
default_permissions = "windows-controlled"

[permissions.windows-controlled.network]
enabled = true

[permissions.windows-controlled.network.domains]
"*.example.com" = "allow"
"blocked.example.com" = "deny"
"#,
        )
        .expect("managed requirements");
        assert!(allowed_hosts_from_managed_requirements(
            &document,
            &ControlledNetworkPolicyRequest::default(),
        )
        .is_err());
    }

    #[test]
    fn rejects_callers_that_try_to_inject_sid_hosts_or_ports() {
        for injected in [
            json!({ "windows_user_sid": "S-1-5-21-1-2-3-4" }),
            json!({ "allowed_hosts": ["evil.example.com"] }),
            json!({ "allowed_ports": [443] }),
        ] {
            assert!(serde_json::from_value::<ControlledNetworkPolicyRequest>(injected).is_err());
        }
    }
}
