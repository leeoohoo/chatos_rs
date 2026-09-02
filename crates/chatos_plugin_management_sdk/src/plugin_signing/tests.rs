// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

use super::*;
use crate::{
    parse_plugin_manifest, plugin_component_descriptors, PluginCatalogRecord,
    PluginComponentSnapshot, PluginLicenseMetadata, PluginNpmPackage, PluginPublisher,
    PluginReleaseRecord,
};

const ARTIFACT_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn verifies_a_canonical_ed25519_release_signature() {
    let fixture = signed_fixture();
    verify_plugin_release_signature(
        fixture.context(),
        &fixture.manifest,
        &fixture.signature,
        &fixture.key,
    )
    .expect("valid release signature");
}

#[test]
fn rejects_artifact_or_version_tampering() {
    let fixture = signed_fixture();
    let tampered_artifact = PluginReleaseVerificationContext {
        artifact_sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ..fixture.context()
    };
    assert!(matches!(
        verify_plugin_release_signature(
            tampered_artifact,
            &fixture.manifest,
            &fixture.signature,
            &fixture.key,
        ),
        Err(PluginSignatureVerificationError::InvalidSignature)
    ));

    let tampered_version = PluginReleaseVerificationContext {
        version: "1.0.1",
        ..fixture.context()
    };
    assert!(matches!(
        verify_plugin_release_signature(
            tampered_version,
            &fixture.manifest,
            &fixture.signature,
            &fixture.key,
        ),
        Err(PluginSignatureVerificationError::InvalidSignature)
    ));
}

#[test]
fn rejects_placeholder_signatures_and_revoked_keys() {
    let mut fixture = signed_fixture();
    fixture.signature.signature_base64 = STANDARD.encode([0_u8; 64]);
    assert!(matches!(
        verify_plugin_release_signature(
            fixture.context(),
            &fixture.manifest,
            &fixture.signature,
            &fixture.key,
        ),
        Err(PluginSignatureVerificationError::InvalidSignature)
    ));

    fixture = signed_fixture();
    fixture.key.revoked_at = Some("2026-07-22T02:00:00Z".to_string());
    assert!(matches!(
        verify_plugin_release_signature(
            fixture.context(),
            &fixture.manifest,
            &fixture.signature,
            &fixture.key,
        ),
        Err(PluginSignatureVerificationError::KeyRevoked)
    ));
}

#[test]
fn verifies_catalog_root_signature_and_rotated_release_key() {
    let fixture = signed_catalog_fixture();
    verify_plugin_catalog_document(
        &fixture.document,
        std::slice::from_ref(&fixture.catalog_key),
    )
    .expect("valid signed Catalog document");
    verify_plugin_catalog_update(
        &fixture.document,
        std::slice::from_ref(&fixture.catalog_key),
        Some("revision-1"),
        Some("2026-07-22T00:00:00Z"),
    )
    .expect("forward Catalog update");
    assert!(matches!(
        verify_plugin_catalog_update(
            &fixture.document,
            std::slice::from_ref(&fixture.catalog_key),
            Some("revision-2"),
            Some("2026-07-22T00:00:00Z"),
        ),
        Err(PluginSignatureVerificationError::InvalidCatalogDocument { .. })
    ));
}

#[test]
fn rejects_catalog_tampering_and_revoked_trust_roots() {
    let mut fixture = signed_catalog_fixture();
    fixture.document.plugins[0].enabled = false;
    assert!(matches!(
        verify_plugin_catalog_document(
            &fixture.document,
            std::slice::from_ref(&fixture.catalog_key)
        ),
        Err(PluginSignatureVerificationError::CatalogHashMismatch)
    ));

    fixture = signed_catalog_fixture();
    fixture.catalog_key.revoked_at = Some("2026-07-22T02:00:00Z".to_string());
    assert!(matches!(
        verify_plugin_catalog_document(&fixture.document, &[fixture.catalog_key]),
        Err(PluginSignatureVerificationError::KeyRevoked)
    ));

    fixture = signed_catalog_fixture();
    fixture.catalog_key.usages = vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()];
    assert!(matches!(
        verify_plugin_catalog_document(&fixture.document, &[fixture.catalog_key]),
        Err(PluginSignatureVerificationError::InvalidCatalogDocument { .. })
    ));
}

#[test]
fn rejects_catalog_release_metadata_that_drifts_from_the_manifest() {
    let mut fixture = signed_catalog_fixture();
    fixture.document.releases[0].components.clear();
    assert!(matches!(
        normalized_plugin_catalog_sha256(&fixture.document),
        Err(PluginSignatureVerificationError::InvalidCatalogDocument { .. })
    ));

    let mut fixture = signed_catalog_fixture();
    fixture.document.component_snapshots.clear();
    assert!(matches!(
        normalized_plugin_catalog_sha256(&fixture.document),
        Err(PluginSignatureVerificationError::InvalidCatalogDocument { .. })
    ));

    let mut fixture = signed_catalog_fixture();
    let release_id = fixture.document.releases[0].id.clone();
    fixture.document.releases[0].revoked_at = Some("2026-07-22T02:00:00Z".to_string());
    fixture.document.revoked_release_ids = vec![release_id];
    assert!(matches!(
        normalized_plugin_catalog_sha256(&fixture.document),
        Err(PluginSignatureVerificationError::InvalidCatalogDocument { .. })
    ));
}

struct SignedFixture {
    manifest: PluginManifest,
    signature: PluginReleaseSignature,
    key: SigningKeyRef,
}

impl SignedFixture {
    fn context(&self) -> PluginReleaseVerificationContext<'_> {
        PluginReleaseVerificationContext {
            plugin_id: "plugin-demo",
            version: self.manifest.version.as_str(),
            marketplace_id: "marketplace-demo",
            publisher_id: "publisher-demo",
            artifact_sha256: ARTIFACT_SHA256,
        }
    }
}

fn signed_fixture() -> SignedFixture {
    let manifest = parse_plugin_manifest(
        r#"{
          "name":"demo-plugin",
          "version":"1.0.0",
          "description":"Demo plugin",
          "author":{"name":"ChatOS"},
          "skills":"./skills",
          "interface":{"displayName":"Demo","shortDescription":"Demo","longDescription":"Demo plugin","developerName":"ChatOS","category":"Developer Tools"}
        }"#,
    )
    .expect("manifest");
    let keypair_bytes = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("test key");
    let keypair = Ed25519KeyPair::from_pkcs8(keypair_bytes.as_ref()).expect("parse test key");
    let mut signature = PluginReleaseSignature {
        key_id: "key-demo".to_string(),
        publisher_id: "publisher-demo".to_string(),
        marketplace_id: "marketplace-demo".to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        signature_base64: String::new(),
        signed_at: "2026-07-22T01:00:00Z".to_string(),
        manifest_sha256: normalized_plugin_manifest_sha256(&manifest).expect("manifest hash"),
    };
    let context = PluginReleaseVerificationContext {
        plugin_id: "plugin-demo",
        version: manifest.version.as_str(),
        marketplace_id: "marketplace-demo",
        publisher_id: "publisher-demo",
        artifact_sha256: ARTIFACT_SHA256,
    };
    let payload = plugin_release_signing_payload(context, &signature).expect("signing payload");
    signature.signature_base64 = STANDARD.encode(keypair.sign(payload.as_slice()).as_ref());
    let key = SigningKeyRef {
        key_id: signature.key_id.clone(),
        publisher_id: signature.publisher_id.clone(),
        algorithm: signature.algorithm.clone(),
        public_key_base64: STANDARD.encode(keypair.public_key().as_ref()),
        usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
        valid_from: "2026-07-22T00:00:00Z".to_string(),
        valid_until: Some("2027-07-22T00:00:00Z".to_string()),
        revoked_at: None,
    };
    SignedFixture {
        manifest,
        signature,
        key,
    }
}

struct SignedCatalogFixture {
    document: PluginCatalogDocument,
    catalog_key: SigningKeyRef,
}

fn signed_catalog_fixture() -> SignedCatalogFixture {
    let manifest = parse_plugin_manifest(
        r#"{
          "name":"demo-plugin",
          "version":"1.0.0",
          "description":"Demo plugin",
          "author":{"name":"Demo Publisher"},
          "skills":"./skills",
          "interface":{"displayName":"Demo","shortDescription":"Demo","longDescription":"Demo plugin","developerName":"Demo Publisher","category":"Developer Tools"}
        }"#,
    )
    .expect("Catalog manifest");
    let catalog_signer = test_signer(
        "catalog-root-v1",
        "marketplace-authority",
        PLUGIN_SIGNING_KEY_USAGE_CATALOG,
    );
    let release_signer = test_signer(
        "publisher-release-v2",
        "publisher-demo",
        PLUGIN_SIGNING_KEY_USAGE_RELEASE,
    );
    let mut release_signature = PluginReleaseSignature {
        key_id: release_signer.key.key_id.clone(),
        publisher_id: "publisher-demo".to_string(),
        marketplace_id: "marketplace-demo".to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        signature_base64: String::new(),
        signed_at: "2026-07-22T01:00:00Z".to_string(),
        manifest_sha256: normalized_plugin_manifest_sha256(&manifest).expect("manifest hash"),
    };
    let release_context = PluginReleaseVerificationContext {
        plugin_id: "plugin-demo",
        version: manifest.version.as_str(),
        marketplace_id: "marketplace-demo",
        publisher_id: "publisher-demo",
        artifact_sha256: ARTIFACT_SHA256,
    };
    let release_payload = plugin_release_signing_payload(release_context, &release_signature)
        .expect("Release payload");
    release_signature.signature_base64 =
        STANDARD.encode(release_signer.keypair.sign(&release_payload).as_ref());
    let release = PluginReleaseRecord {
        id: "release-demo-1-0-0".to_string(),
        plugin_id: "plugin-demo".to_string(),
        version: manifest.version.clone(),
        manifest_schema_version: manifest.schema_version,
        normalized_manifest: manifest.clone(),
        npm_package: PluginNpmPackage {
            name: "demo-plugin".to_string(),
            version: manifest.version.clone(),
            integrity: "sha512-ZGVtby1pbnRlZ3JpdHk=".to_string(),
        },
        artifact_ref: "https://registry.npmjs.org/demo-plugin/-/demo-plugin-1.0.0.tgz".to_string(),
        artifact_sha256: ARTIFACT_SHA256.to_string(),
        signature: release_signature,
        sbom_ref: Some("./sbom.json".to_string()),
        supported_platforms: manifest.dependencies.supported_platforms.clone(),
        components: plugin_component_descriptors(&manifest),
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        release_channel: "stable".to_string(),
        published_at: "2026-07-22T01:00:00Z".to_string(),
        revoked_at: None,
    };
    let plugin = PluginCatalogRecord {
        id: "plugin-demo".to_string(),
        plugin_key: "demo-plugin@marketplace-demo".to_string(),
        marketplace_id: "marketplace-demo".to_string(),
        owner_user_id: None,
        name: manifest.name.clone(),
        display_name: manifest.interface.display_name.clone(),
        description: manifest.description.clone(),
        publisher: PluginPublisher {
            id: "publisher-demo".to_string(),
            name: "Demo Publisher".to_string(),
            website: Some("https://plugins.example.com".to_string()),
            verified: true,
        },
        interface: manifest.interface.clone(),
        keywords: Vec::new(),
        visibility: "public".to_string(),
        featured: false,
        enabled: true,
        has_ui: false,
        latest_release_id: release.id.clone(),
        license: PluginLicenseMetadata {
            license_id: "MIT".to_string(),
            license_url: None,
            redistributable: true,
            reviewed_at: Some("2026-07-22T00:00:00Z".to_string()),
        },
        created_at: "2026-07-22T00:00:00Z".to_string(),
        updated_at: "2026-07-22T01:00:00Z".to_string(),
    };
    let component_snapshots = release
        .components
        .iter()
        .cloned()
        .map(|component| PluginComponentSnapshot {
            plugin_id: release.plugin_id.clone(),
            release_id: release.id.clone(),
            component,
            content_sha256: "b".repeat(64),
        })
        .collect();
    let mut document = PluginCatalogDocument {
        schema_version: PLUGIN_CATALOG_SCHEMA_VERSION_V1,
        marketplace_id: "marketplace-demo".to_string(),
        revision: "revision-2".to_string(),
        issued_at: "2026-07-22T01:00:00Z".to_string(),
        signing_keys: vec![catalog_signer.key.clone(), release_signer.key],
        plugins: vec![plugin],
        releases: vec![release],
        component_snapshots,
        revoked_release_ids: Vec::new(),
        signature: PluginCatalogSignature {
            key_id: catalog_signer.key.key_id.clone(),
            marketplace_id: "marketplace-demo".to_string(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            signature_base64: String::new(),
            signed_at: "2026-07-22T01:00:00Z".to_string(),
            catalog_sha256: "0".repeat(64),
        },
    };
    document.signature.catalog_sha256 =
        normalized_plugin_catalog_sha256(&document).expect("Catalog hash");
    let context = PluginCatalogVerificationContext {
        marketplace_id: document.marketplace_id.as_str(),
        revision: document.revision.as_str(),
        issued_at: document.issued_at.as_str(),
    };
    let payload =
        plugin_catalog_signing_payload(context, &document.signature).expect("Catalog payload");
    document.signature.signature_base64 =
        STANDARD.encode(catalog_signer.keypair.sign(&payload).as_ref());
    SignedCatalogFixture {
        document,
        catalog_key: catalog_signer.key,
    }
}

struct TestSigner {
    keypair: Ed25519KeyPair,
    key: SigningKeyRef,
}

fn test_signer(key_id: &str, publisher_id: &str, usage: &str) -> TestSigner {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("test key");
    let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parse test key");
    let key = SigningKeyRef {
        key_id: key_id.to_string(),
        publisher_id: publisher_id.to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        public_key_base64: STANDARD.encode(keypair.public_key().as_ref()),
        usages: vec![usage.to_string()],
        valid_from: "2026-01-01T00:00:00Z".to_string(),
        valid_until: Some("2027-01-01T00:00:00Z".to_string()),
        revoked_at: None,
    };
    TestSigner { keypair, key }
}
