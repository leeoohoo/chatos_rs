// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::{engine::general_purpose::STANDARD, Engine as _};

use super::*;
use chatos_plugin_management_sdk::{
    PluginCatalogSignature, PLUGIN_CATALOG_SCHEMA_VERSION_V1, PLUGIN_SIGNATURE_ALGORITHM_ED25519,
};

#[test]
fn catalog_urls_reject_non_https_credentials_and_fragments() {
    assert!(validate_catalog_url("https://plugins.example.com/catalog.json").is_ok());
    assert!(validate_catalog_url("http://plugins.example.com/catalog.json").is_err());
    assert!(validate_catalog_url("https://user@plugins.example.com/catalog.json").is_err());
    assert!(validate_catalog_url("https://plugins.example.com/catalog.json#v1").is_err());
}

#[test]
fn catalog_fetch_rejects_private_and_special_networks() {
    for value in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.1.1",
        "100.64.0.1",
        "198.18.0.1",
        "::1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
    ] {
        let ip: IpAddr = value.parse().expect("test IP");
        assert!(!is_public_ip(ip), "{value}");
    }
    assert!(is_public_ip("8.8.8.8".parse().expect("public IPv4")));
    assert!(is_public_ip(
        "2606:4700:4700::1111".parse().expect("public IPv6")
    ));
}

#[test]
fn signing_key_rotation_requires_revocation_before_removal() {
    let key = signing_key("root-v1", None);
    let previous = catalog_with_keys("revision-1", vec![key.clone()]);
    let next = catalog_with_keys("revision-2", Vec::new());
    assert!(validate_marketplace_signing_key_progression(
        previous.signing_keys.as_slice(),
        next.signing_keys.as_slice(),
    )
    .is_err());

    let revoked = signing_key("root-v1", Some("2026-07-25T01:00:00Z"));
    let previous = catalog_with_keys("revision-1", vec![revoked]);
    assert!(validate_marketplace_signing_key_progression(
        previous.signing_keys.as_slice(),
        next.signing_keys.as_slice(),
    )
    .is_ok());
}

fn signing_key(key_id: &str, revoked_at: Option<&str>) -> SigningKeyRef {
    SigningKeyRef {
        key_id: key_id.to_string(),
        publisher_id: "marketplace-authority".to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        public_key_base64: STANDARD.encode([1_u8; 32]),
        usages: vec![PLUGIN_SIGNING_KEY_USAGE_CATALOG.to_string()],
        valid_from: "2026-01-01T00:00:00Z".to_string(),
        valid_until: Some("2027-01-01T00:00:00Z".to_string()),
        revoked_at: revoked_at.map(ToOwned::to_owned),
    }
}

fn catalog_with_keys(revision: &str, signing_keys: Vec<SigningKeyRef>) -> PluginCatalogDocument {
    PluginCatalogDocument {
        schema_version: PLUGIN_CATALOG_SCHEMA_VERSION_V1,
        marketplace_id: "marketplace-demo".to_string(),
        revision: revision.to_string(),
        issued_at: "2026-07-25T00:00:00Z".to_string(),
        signing_keys,
        plugins: Vec::new(),
        releases: Vec::new(),
        component_snapshots: Vec::new(),
        revoked_release_ids: Vec::new(),
        signature: PluginCatalogSignature {
            key_id: "root-v1".to_string(),
            marketplace_id: "marketplace-demo".to_string(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            signature_base64: STANDARD.encode([0_u8; 64]),
            signed_at: "2026-07-25T00:00:00Z".to_string(),
            catalog_sha256: "0".repeat(64),
        },
    }
}
