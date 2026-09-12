// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_capability::{
    plugin_release_signing_payload, verify_signed_plugin_manifest,
    PluginCapabilityVerificationError, PluginReleaseSignature, PluginReleaseVerificationContext,
    TrustedPluginSigningKey, MAX_SIGNED_MANIFEST_BYTES, PLUGIN_SIGNATURE_ALGORITHM_ED25519,
    PLUGIN_SIGNING_KEY_USAGE_RELEASE,
};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::json;
use sha2::{Digest, Sha256};

const ARTIFACT_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn context<'a>(
    plugin_id: &'a str,
    version: &'a str,
    marketplace_id: &'a str,
    publisher_id: &'a str,
    artifact_sha256: &'a str,
) -> PluginReleaseVerificationContext<'a> {
    PluginReleaseVerificationContext {
        plugin_id,
        version,
        marketplace_id,
        publisher_id,
        artifact_sha256,
    }
}

fn valid_context() -> PluginReleaseVerificationContext<'static> {
    context(
        "plugin-demo",
        "1.0.0",
        "marketplace-1",
        "publisher-1",
        ARTIFACT_SHA256,
    )
}

fn manifest_bytes() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 3,
        "name": "demo-plugin",
        "version": "1.0.0",
        "mcpServers": [{
            "transport": "stdio",
            "component_key": "demo-mcp",
            "bin": "demo-plugin-bin",
            "args": ["serve"],
            "env": {"API_TOKEN": "${credential:api-token}"}
        }],
        "dependencies": {"supportedPlatforms": ["macos", "windows", "linux"]},
        "permissions": [{
            "permission": "process.spawn",
            "required": true,
            "components": ["demo-mcp"]
        }]
    }))
    .unwrap()
}

fn signed_release(manifest: &[u8]) -> (String, PluginReleaseSignature, TrustedPluginSigningKey) {
    let keypair_bytes = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let keypair = Ed25519KeyPair::from_pkcs8(keypair_bytes.as_ref()).unwrap();
    let mut signature = PluginReleaseSignature {
        key_id: "key-1".to_string(),
        publisher_id: "publisher-1".to_string(),
        marketplace_id: "marketplace-1".to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        signature_base64: String::new(),
        signed_at: "2026-09-12T00:00:00Z".to_string(),
        manifest_sha256: format!("{:x}", Sha256::digest(manifest)),
    };
    let payload = plugin_release_signing_payload(valid_context(), &signature).unwrap();
    signature.signature_base64 = STANDARD.encode(keypair.sign(&payload).as_ref());
    let key = TrustedPluginSigningKey {
        key_id: "key-1".to_string(),
        publisher_id: "publisher-1".to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        public_key_base64: STANDARD.encode(keypair.public_key().as_ref()),
        usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
        valid_from: "2026-09-11T00:00:00Z".to_string(),
        valid_until: Some("2027-09-12T00:00:00Z".to_string()),
        revoked_at: None,
    };
    (STANDARD.encode(manifest), signature, key)
}

#[test]
fn verifies_the_exact_signed_manifest_bytes() {
    let manifest = manifest_bytes();
    let (encoded, signature, key) = signed_release(&manifest);

    let verified = verify_signed_plugin_manifest(valid_context(), &encoded, &signature, &key)
        .expect("valid signed Release must verify");

    assert_eq!(verified.name, "demo-plugin");
    assert_eq!(verified.mcp_servers.len(), 1);
}

#[test]
fn rejects_any_manifest_byte_change() {
    let manifest = manifest_bytes();
    let (_, signature, key) = signed_release(&manifest);
    let mut changed = manifest;
    changed.push(b' ');

    assert_eq!(
        verify_signed_plugin_manifest(valid_context(), &STANDARD.encode(changed), &signature, &key,),
        Err(PluginCapabilityVerificationError::ManifestDigestMismatch)
    );
}

#[test]
fn rejects_release_identity_drift() {
    let manifest = manifest_bytes();
    let (encoded, signature, key) = signed_release(&manifest);
    let changed_artifact = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let contexts = [
        context(
            "other-plugin",
            "1.0.0",
            "marketplace-1",
            "publisher-1",
            ARTIFACT_SHA256,
        ),
        context(
            "plugin-demo",
            "2.0.0",
            "marketplace-1",
            "publisher-1",
            ARTIFACT_SHA256,
        ),
        context(
            "plugin-demo",
            "1.0.0",
            "marketplace-2",
            "publisher-1",
            ARTIFACT_SHA256,
        ),
        context(
            "plugin-demo",
            "1.0.0",
            "marketplace-1",
            "publisher-2",
            ARTIFACT_SHA256,
        ),
        context(
            "plugin-demo",
            "1.0.0",
            "marketplace-1",
            "publisher-1",
            changed_artifact,
        ),
    ];

    for changed in contexts {
        assert!(verify_signed_plugin_manifest(changed, &encoded, &signature, &key).is_err());
    }

    let mut changed_key = key.clone();
    changed_key.key_id = "key-2".to_string();
    assert!(
        verify_signed_plugin_manifest(valid_context(), &encoded, &signature, &changed_key,)
            .is_err()
    );
}

#[test]
fn rejects_revoked_unauthorized_and_out_of_window_keys() {
    let manifest = manifest_bytes();
    let (encoded, signature, key) = signed_release(&manifest);

    let mut revoked = key.clone();
    revoked.revoked_at = Some("2026-09-12T01:00:00Z".to_string());
    assert_eq!(
        verify_signed_plugin_manifest(valid_context(), &encoded, &signature, &revoked),
        Err(PluginCapabilityVerificationError::RevokedSigningKey)
    );

    let mut unauthorized = key.clone();
    unauthorized.usages.clear();
    assert_eq!(
        verify_signed_plugin_manifest(valid_context(), &encoded, &signature, &unauthorized),
        Err(PluginCapabilityVerificationError::UnauthorizedSigningKey)
    );

    let mut future = key;
    future.valid_from = "2026-09-13T00:00:00Z".to_string();
    assert_eq!(
        verify_signed_plugin_manifest(valid_context(), &encoded, &signature, &future),
        Err(PluginCapabilityVerificationError::OutsideSigningKeyValidity)
    );
}

#[test]
fn rejects_oversized_and_malformed_manifests_without_leaking_payloads() {
    let manifest = manifest_bytes();
    let (_, signature, key) = signed_release(&manifest);
    let oversized = STANDARD.encode(vec![b'x'; MAX_SIGNED_MANIFEST_BYTES + 1]);
    let oversized_error =
        verify_signed_plugin_manifest(valid_context(), &oversized, &signature, &key).unwrap_err();
    assert_eq!(
        oversized_error,
        PluginCapabilityVerificationError::ManifestTooLarge {
            maximum: MAX_SIGNED_MANIFEST_BYTES
        }
    );

    let private_marker = br#"{"privateToken":"must-not-appear"}"#;
    let (encoded, malformed_signature, malformed_key) = signed_release(private_marker);
    let malformed_error = verify_signed_plugin_manifest(
        valid_context(),
        &encoded,
        &malformed_signature,
        &malformed_key,
    )
    .unwrap_err();
    let diagnostic = format!("{malformed_error:?} {malformed_error}");
    assert!(!diagnostic.contains("must-not-appear"));
    assert!(!diagnostic.contains(&encoded));
}
