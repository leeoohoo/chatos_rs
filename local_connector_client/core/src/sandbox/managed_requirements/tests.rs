// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

use chatos_sandbox_contract::{ManagedRequirementsBundleLayer, ManagedRequirementsBundlePayload};

use super::*;
use crate::sandbox::managed_requirements_cache::store_test_bundle_with_signer;

struct TestBundle {
    identity: ManagedRequirementsIdentity,
    client_config: ManagedRequirementsClientConfig,
    keypair: Ed25519KeyPair,
}

impl TestBundle {
    fn new() -> Self {
        let service_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(service_pkcs8.as_ref()).unwrap();
        let service_public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
        );
        let device_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let device_keypair = Ed25519KeyPair::from_pkcs8(device_pkcs8.as_ref()).unwrap();
        Self {
            identity: ManagedRequirementsIdentity {
                cloud_base_url: "https://connector.example.test".to_string(),
                owner_user_id: "user-1".to_string(),
                device_id: "device-1".to_string(),
                device_public_key: format!(
                    "ed25519:{}",
                    URL_SAFE_NO_PAD.encode(device_keypair.public_key().as_ref())
                ),
            },
            client_config: ManagedRequirementsClientConfig {
                schema_version: CLIENT_CONFIG_SCHEMA_VERSION,
                trusted_signing_keys: BTreeMap::from([(
                    "service-key-1".to_string(),
                    service_public_key,
                )]),
                fetch_attempts: 3,
                retry_delay_ms: 50,
                request_timeout_ms: 1_000,
                minimum_bundle_issued_at: None,
            },
            keypair,
        }
    }

    fn bundle(&self, now: DateTime<Utc>) -> ManagedRequirementsBundle {
        let requirements_toml = r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
"#;
        let payload = ManagedRequirementsBundlePayload {
            schema_version: MANAGED_REQUIREMENTS_BUNDLE_SCHEMA_VERSION,
            key_id: "service-key-1".to_string(),
            cloud_base_url: self.identity.cloud_base_url.clone(),
            owner_user_id: self.identity.owner_user_id.clone(),
            device_id: self.identity.device_id.clone(),
            device_public_key: self.identity.device_public_key.clone(),
            issued_at: (now - Duration::minutes(1)).to_rfc3339(),
            expires_at: (now + Duration::hours(1)).to_rfc3339(),
            layers: vec![ManagedRequirementsBundleLayer {
                policy_id: "policy-1".to_string(),
                policy_version: 1,
                assignment_id: "assignment-1".to_string(),
                assignment_scope: "global".to_string(),
                requirements_toml: requirements_toml.to_string(),
                requirements_sha256: requirements_digest(requirements_toml.as_bytes()),
            }],
        };
        let signed = managed_requirements_bundle_signature_payload(&payload).unwrap();
        ManagedRequirementsBundle {
            payload,
            signature: URL_SAFE_NO_PAD.encode(self.keypair.sign(signed.as_slice()).as_ref()),
        }
    }

    fn resign(&self, bundle: &mut ManagedRequirementsBundle) {
        let signed = managed_requirements_bundle_signature_payload(&bundle.payload).unwrap();
        bundle.signature = URL_SAFE_NO_PAD.encode(self.keypair.sign(signed.as_slice()).as_ref());
    }
}

#[test]
fn valid_service_signed_bundle_is_accepted() {
    let test = TestBundle::new();
    let now = Utc::now();

    let verified = verify_bundle(&test.bundle(now), &test.identity, &test.client_config, now)
        .expect("valid bundle");

    assert_eq!(
        verified
            .document
            .as_ref()
            .and_then(|document| document.default_permissions.as_deref()),
        Some(":read-only")
    );
}

#[test]
fn service_signature_content_expiry_and_identity_are_fail_closed() {
    let test = TestBundle::new();
    let now = Utc::now();

    let mut signature = test.bundle(now);
    signature.signature.push('A');
    assert!(format!(
        "{:#}",
        verify_bundle(&signature, &test.identity, &test.client_config, now).unwrap_err()
    )
    .contains("signature"));

    let mut content = test.bundle(now);
    content.payload.layers[0].requirements_toml =
        "default_permissions = \":workspace\"".to_string();
    test.resign(&mut content);
    assert!(format!(
        "{:#}",
        verify_bundle(&content, &test.identity, &test.client_config, now).unwrap_err()
    )
    .contains("digest"));

    let mut expired = test.bundle(now);
    expired.payload.issued_at = (now - Duration::hours(2)).to_rfc3339();
    expired.payload.expires_at = (now - Duration::hours(1)).to_rfc3339();
    test.resign(&mut expired);
    assert!(format!(
        "{:#}",
        verify_bundle(&expired, &test.identity, &test.client_config, now).unwrap_err()
    )
    .contains("expired"));

    let mut wrong_identity = test.identity.clone();
    wrong_identity.owner_user_id = "user-2".to_string();
    assert!(format!(
        "{:#}",
        verify_bundle(&test.bundle(now), &wrong_identity, &test.client_config, now).unwrap_err()
    )
    .contains("current pairing"));
}

#[test]
fn untrusted_service_key_is_rejected() {
    let test = TestBundle::new();
    let now = Utc::now();
    let mut config = test.client_config.clone();
    config.trusted_signing_keys.clear();

    let error = verify_bundle(&test.bundle(now), &test.identity, &config, now)
        .expect_err("untrusted key must fail");

    assert!(error.to_string().contains("not trusted"));
}

#[test]
fn higher_precedence_bundle_layers_override_lower_layers() {
    let test = TestBundle::new();
    let now = Utc::now();
    let mut bundle = test.bundle(now);
    let requirements_toml = r#"
default_permissions = ":workspace"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#;
    bundle.payload.layers.push(ManagedRequirementsBundleLayer {
        policy_id: "policy-user".to_string(),
        policy_version: 2,
        assignment_id: "assignment-user".to_string(),
        assignment_scope: "user".to_string(),
        requirements_toml: requirements_toml.to_string(),
        requirements_sha256: requirements_digest(requirements_toml.as_bytes()),
    });
    test.resign(&mut bundle);

    let verified = verify_bundle(&bundle, &test.identity, &test.client_config, now).unwrap();

    assert_eq!(
        verified
            .document
            .and_then(|document| document.default_permissions),
        Some(":workspace".to_string())
    );
}

#[test]
fn empty_layers_are_an_explicit_valid_no_requirements_bundle() {
    let test = TestBundle::new();
    let now = Utc::now();
    let mut bundle = test.bundle(now);
    bundle.payload.layers.clear();
    test.resign(&mut bundle);

    let verified = verify_bundle(&bundle, &test.identity, &test.client_config, now).unwrap();

    assert!(verified.document.is_none());
}

#[test]
fn managed_layer_rejects_unrelated_top_level_keys() {
    let test = TestBundle::new();
    let now = Utc::now();
    let mut bundle = test.bundle(now);
    bundle.payload.layers[0].requirements_toml = "model = \"gpt-test\"".to_string();
    bundle.payload.layers[0].requirements_sha256 =
        requirements_digest(bundle.payload.layers[0].requirements_toml.as_bytes());
    test.resign(&mut bundle);

    let error = verify_bundle(&bundle, &test.identity, &test.client_config, now)
        .expect_err("unrelated managed policy keys must fail closed");

    assert!(format!("{error:#}").contains("unsupported managed requirements"));
}

#[test]
fn signing_keys_can_overlap_during_rotation_and_removed_keys_are_rejected() {
    let test = TestBundle::new();
    let now = Utc::now();
    let new_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let new_keypair = Ed25519KeyPair::from_pkcs8(new_pkcs8.as_ref()).unwrap();
    let new_public_key = format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(new_keypair.public_key().as_ref())
    );
    let mut config = test.client_config.clone();
    config
        .trusted_signing_keys
        .insert("service-key-2".to_string(), new_public_key);
    let old_bundle = test.bundle(now);
    let mut new_bundle = old_bundle.clone();
    new_bundle.payload.key_id = "service-key-2".to_string();
    let signed = managed_requirements_bundle_signature_payload(&new_bundle.payload).unwrap();
    new_bundle.signature = URL_SAFE_NO_PAD.encode(new_keypair.sign(signed.as_slice()).as_ref());

    assert!(verify_bundle(&old_bundle, &test.identity, &config, now).is_ok());
    assert!(verify_bundle(&new_bundle, &test.identity, &config, now).is_ok());
    config.trusted_signing_keys.remove("service-key-1");
    assert!(verify_bundle(&old_bundle, &test.identity, &config, now).is_err());
    assert!(verify_bundle(&new_bundle, &test.identity, &config, now).is_ok());
}

#[test]
fn trust_root_minimum_issue_time_and_cached_issue_time_prevent_rollback() {
    let test = TestBundle::new();
    let now = Utc::now();
    let mut config = test.client_config.clone();
    config.minimum_bundle_issued_at = Some(now.to_rfc3339());

    let error = verify_bundle(&test.bundle(now), &test.identity, &config, now)
        .expect_err("bundle older than trust-root floor must fail");
    assert!(error.to_string().contains("rollback detected"));
    assert!(ensure_bundle_not_older(now - Duration::seconds(1), now).is_err());
    assert!(ensure_bundle_not_older(now, now).is_ok());
}

#[test]
fn client_config_rejects_unbounded_retry_or_timeout_values() {
    let test = TestBundle::new();
    let mut config = test.client_config;
    config.fetch_attempts = 0;
    assert!(validate_client_config(&config).is_err());
    config.fetch_attempts = 3;
    config.request_timeout_ms = 60_000;
    assert!(validate_client_config(&config).is_err());
}

fn test_state_path(label: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "chatos-managed-startup-{label}-{}",
            uuid::Uuid::new_v4()
        ))
        .join("state.json")
}

fn paired_state(identity: &ManagedRequirementsIdentity) -> LocalState {
    LocalState {
        paired_cloud_base_url: Some(identity.cloud_base_url.clone()),
        paired_user_id: Some(identity.owner_user_id.clone()),
        device_id: Some(identity.device_id.clone()),
        device_public_key: Some(identity.device_public_key.clone()),
        ..Default::default()
    }
}

fn unavailable_connector_config(state_path: PathBuf) -> ClientConfig {
    ClientConfig {
        cloud_base_url: "https://connector.example.test".to_string(),
        access_token: "test-token".to_string(),
        device_name: "test-device".to_string(),
        public_key: None,
        workspace_path: None,
        workspace_alias: None,
        state_path,
    }
}

#[tokio::test]
async fn valid_cache_is_used_without_waiting_for_failed_network_refresh() {
    let test = TestBundle::new();
    let now = Utc::now();
    let state_path = test_state_path("cache-fallback");
    let bundle = test.bundle(now);
    let device_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let device_keypair = Ed25519KeyPair::from_pkcs8(device_pkcs8.as_ref()).unwrap();
    let mut identity = test.identity.clone();
    identity.device_public_key = format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(device_keypair.public_key().as_ref())
    );
    let mut bundle = bundle;
    bundle.payload.device_public_key = identity.device_public_key.clone();
    let signed = managed_requirements_bundle_signature_payload(&bundle.payload).unwrap();
    bundle.signature = URL_SAFE_NO_PAD.encode(test.keypair.sign(signed.as_slice()).as_ref());
    store_test_bundle_with_signer(state_path.as_path(), &identity, &bundle, |payload| {
        URL_SAFE_NO_PAD.encode(device_keypair.sign(payload).as_ref())
    })
    .unwrap();
    let state = paired_state(&identity);
    let connector_config = unavailable_connector_config(state_path.clone());
    let http_client = reqwest::Client::new();

    let resolved = resolve_startup_managed_requirements(
        &http_client,
        state_path.as_path(),
        &state,
        Some(&connector_config),
        Some(test.client_config),
    )
    .await
    .expect("valid cache should be used immediately");

    assert_eq!(
        resolved
            .document
            .as_ref()
            .and_then(|document| document.default_permissions.as_deref()),
        Some(":read-only")
    );
    assert!(resolved.background_refresh.is_some());
    let _ = fs::remove_dir_all(state_path.parent().unwrap());
}

#[tokio::test]
async fn expired_trusted_cache_prevents_an_older_fetched_bundle_rollback() {
    let mut test = TestBundle::new();
    test.client_config.fetch_attempts = 1;
    let now = Utc::now();
    let state_path = test_state_path("rollback-cache");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cloud_base_url = format!("http://{}", listener.local_addr().unwrap());
    test.identity.cloud_base_url = cloud_base_url.clone();
    let device_pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let device_keypair = Ed25519KeyPair::from_pkcs8(device_pkcs8.as_ref()).unwrap();
    test.identity.device_public_key = format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(device_keypair.public_key().as_ref())
    );

    let mut cached = test.bundle(now);
    cached.payload.issued_at = (now - Duration::minutes(30)).to_rfc3339();
    cached.payload.expires_at = (now - Duration::minutes(1)).to_rfc3339();
    test.resign(&mut cached);
    store_test_bundle_with_signer(state_path.as_path(), &test.identity, &cached, |payload| {
        URL_SAFE_NO_PAD.encode(device_keypair.sign(payload).as_ref())
    })
    .unwrap();

    let mut fetched = test.bundle(now);
    fetched.payload.issued_at = (now - Duration::hours(1)).to_rfc3339();
    fetched.payload.expires_at = (now + Duration::hours(1)).to_rfc3339();
    test.resign(&mut fetched);
    let app = axum::Router::new().route(
        "/api/local-connectors/devices/{id}/managed-requirements",
        axum::routing::get(move || {
            let fetched = fetched.clone();
            async move { axum::Json(fetched) }
        }),
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let state = paired_state(&test.identity);
    let mut connector_config = unavailable_connector_config(state_path.clone());
    connector_config.cloud_base_url = cloud_base_url;
    let http_client = reqwest::Client::new();

    let error = resolve_startup_managed_requirements(
        &http_client,
        state_path.as_path(),
        &state,
        Some(&connector_config),
        Some(test.client_config),
    )
    .await
    .err()
    .expect("an older fetched bundle must not replace a trusted expired cache");

    assert!(format!("{error:#}").contains("rollback detected"));
    assert_eq!(
        load_cached_bundle(state_path.as_path(), &test.identity)
            .unwrap()
            .unwrap()
            .payload
            .issued_at,
        cached.payload.issued_at
    );
    server.abort();
    let _ = fs::remove_dir_all(state_path.parent().unwrap());
}

#[tokio::test]
async fn failed_fetch_without_valid_cache_is_fail_closed() {
    let mut test = TestBundle::new();
    test.client_config.fetch_attempts = 1;
    test.client_config.request_timeout_ms = 1_000;
    let state_path = test_state_path("no-cache");
    let state = paired_state(&test.identity);
    let mut connector_config = unavailable_connector_config(state_path.clone());
    connector_config.cloud_base_url = "http://127.0.0.1:9".to_string();
    let mut state = state;
    state.paired_cloud_base_url = Some(connector_config.cloud_base_url.clone());
    let http_client = reqwest::Client::new();

    let error = resolve_startup_managed_requirements(
        &http_client,
        state_path.as_path(),
        &state,
        Some(&connector_config),
        Some(test.client_config),
    )
    .await
    .err()
    .expect("missing cache and failed fetch must block startup");

    assert!(format!("{error:#}").contains("no valid managed requirements cache"));
    let _ = fs::remove_dir_all(state_path.parent().unwrap());
}
