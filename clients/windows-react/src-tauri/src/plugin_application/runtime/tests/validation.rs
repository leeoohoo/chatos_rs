use super::*;

#[test]
fn context_selection_follows_the_macos_application_contract() {
    let mut fixture = RuntimeFixture::new();
    let runtime_root = fixture
        .app_data
        .join("NativeConnector")
        .join("PluginRuntime");

    assert!(resolve_host_context(
        &runtime_root,
        "owner-1",
        &fixture.resolved.application,
        &PluginApplicationContextSelection::Device,
    )
    .is_err());

    fixture.resolved.application.context_scope = Some("project".into());
    fixture.resolved.application.missing_context = Some("device".into());
    assert!(resolve_host_context(
        &runtime_root,
        "owner-1",
        &fixture.resolved.application,
        &PluginApplicationContextSelection::None,
    )
    .is_err());
    let device = resolve_host_context(
        &runtime_root,
        "owner-1",
        &fixture.resolved.application,
        &PluginApplicationContextSelection::Device,
    )
    .unwrap();
    assert_eq!(device.device_id, "device-1");
    assert!(device.project_id.is_none());
    assert!(resolve_host_context(
        &runtime_root,
        "owner-2",
        &fixture.resolved.application,
        &PluginApplicationContextSelection::Device,
    )
    .is_err());
}

#[test]
fn context_storage_is_owner_plugin_and_project_scoped() {
    let mut fixture = RuntimeFixture::new();
    fixture.resolved.manifest.runtime_context = Some(PluginRuntimeContext {
        scope: "project".into(),
        components: vec!["plugin-workbench".into()],
        required: vec!["project.id".into(), "workspace.root".into()],
        optional: vec!["workspace.id".into()],
        storage_isolation: "project".into(),
        missing_context: "reject".into(),
    });
    let first_project = ResolvedHostContext {
        owner_user_id: "owner-1".into(),
        device_id: "device-1".into(),
        workspace_id: Some("workspace-1".into()),
        workspace_root: Some(fixture.installation.clone()),
        project_id: Some("project-1".into()),
        project_name: Some("First".into()),
    };
    let second_project = ResolvedHostContext {
        project_id: Some("project-2".into()),
        project_name: Some("Second".into()),
        ..first_project.clone()
    };

    let first = resolve_runtime_context(
        &fixture.app_data.join("NativeConnector/PluginRuntime"),
        &fixture.resolved,
        &first_project,
    )
    .unwrap();
    let second = resolve_runtime_context(
        &fixture.app_data.join("NativeConnector/PluginRuntime"),
        &fixture.resolved,
        &second_project,
    )
    .unwrap();

    assert_ne!(first.scope_key, second.scope_key);
    assert_ne!(first.data_directory, second.data_directory);
    assert_eq!(first.environment["CHATOS_PROJECT_ID"], "project-1");
    assert_eq!(first.environment["CHATOS_WORKSPACE_ID"], "workspace-1");
    assert_eq!(first.environment["CHATOS_PROJECT_NAME"], "First");
    assert!(first.data_directory.starts_with(
        fixture
            .app_data
            .join("NativeConnector/PluginRuntime")
            .canonicalize()
            .unwrap()
    ));
}

#[test]
fn process_environment_never_inherits_plugin_secret_references() {
    let fixture = RuntimeFixture::new();
    let host = ResolvedHostContext {
        owner_user_id: "owner-1".into(),
        device_id: "device-1".into(),
        workspace_id: None,
        workspace_root: None,
        project_id: None,
        project_name: None,
    };
    let context = resolve_runtime_context(
        &fixture.app_data.join("NativeConnector/PluginRuntime"),
        &fixture.resolved,
        &host,
    )
    .unwrap();
    let prepared = prepare_runtime_process(&fixture.resolved, &context).unwrap();

    assert!(!prepared.environment.contains_key("API_TOKEN"));
    assert_eq!(
        prepared.environment["CHATOS_PLUGIN_SANDBOX"],
        if cfg!(windows) {
            "windows-appcontainer"
        } else {
            "windows-job-lifecycle"
        }
    );
    assert_eq!(prepared.environment["CHATOS_PLUGIN_APP_HOST"], "127.0.0.1");
    assert_eq!(
        prepared.environment["CHATOS_PLUGIN_APP_HEALTH_PATH"],
        "/api/health"
    );
    assert!(valid_authentication_value(
        &prepared.environment["CHATOS_PLUGIN_APP_TOKEN"]
    ));
}

#[test]
fn package_bin_must_remain_a_non_linked_installed_file() {
    let fixture = RuntimeFixture::new();
    let outside = fixture.app_data.join("outside.exe");
    fs::write(&outside, "outside").unwrap();
    fs::write(
        fixture.installation.join("package.json"),
        serde_json::to_vec(&json!({
            "name": "plugin-runtime-fixture",
            "bin": {"fixture": "../../../../outside.exe"}
        }))
        .unwrap(),
    )
    .unwrap();

    assert!(resolve_package_executable(&fixture.installation, "fixture").is_err());
}

#[cfg(unix)]
#[test]
fn package_bin_symlink_is_rejected_even_when_its_target_is_inside_the_installation() {
    use std::os::unix::fs::symlink;

    let fixture = RuntimeFixture::new();
    let target = fixture.installation.join("real-bin");
    fs::write(&target, "#!/bin/sh\nexit 0\n").unwrap();
    symlink(&target, fixture.installation.join("linked-bin")).unwrap();
    fs::write(
        fixture.installation.join("package.json"),
        serde_json::to_vec(&json!({
            "name": "plugin-runtime-fixture",
            "bin": {"fixture": "linked-bin"}
        }))
        .unwrap(),
    )
    .unwrap();

    assert!(resolve_package_executable(&fixture.installation, "fixture").is_err());
}

#[test]
fn storage_partition_is_stable_without_exposing_scope_identity() {
    let first = storage_partition_id("plugin:app", "owner:secret-project");
    let second = storage_partition_id("plugin:app", "owner:secret-project");

    assert_eq!(first, second);
    assert_eq!(first.len(), 36);
    assert!(!first.contains("secret-project"));
}

#[test]
fn bridge_envelope_is_bounded_and_accepts_only_an_exact_loopback_origin() {
    let request = PluginApplicationBridgeRequest {
        owner_user_id: "owner-1".into(),
        presentation_id: "presentation-1".into(),
        generation: 1,
        session_id: "session-1".into(),
        host_session_nonce: "a".repeat(64),
        origin: "http://127.0.0.1:43210".into(),
        request_id: "request-1".into(),
        method: "host.context.read".into(),
        payload: json!({}),
    };
    assert!(validate_bridge_request(&request).is_ok());

    for invalid_origin in [
        "http://localhost:43210",
        "https://127.0.0.1:43210",
        "http://127.0.0.1:43210/path",
        "http://user@127.0.0.1:43210",
    ] {
        let mut invalid = request.clone();
        invalid.origin = invalid_origin.into();
        assert!(validate_bridge_request(&invalid).is_err());
    }
    let mut oversized = request;
    oversized.payload = json!({"value": "x".repeat(MAXIMUM_BRIDGE_REQUEST_BYTES)});
    assert!(validate_bridge_request(&oversized).is_err());
}

#[test]
fn loopback_proxy_header_gate_rejects_connect_and_absolute_targets() {
    assert!(valid_proxy_request(
        b"GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
    ));
    assert!(!valid_proxy_request(
        b"CONNECT 10.0.0.1:443 HTTP/1.1\r\n\r\n"
    ));
    assert!(!valid_proxy_request(
        b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n"
    ));
    assert!(valid_proxy_response(b"HTTP/1.1 200 OK\r\n\r\n"));
    assert!(!valid_proxy_response(b"not-http\r\n\r\n"));

    let token = "a".repeat(43);
    let bootstrap = format!(
        "GET {PLUGIN_SESSION_BOOTSTRAP_PATH}?token={token} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
    );
    assert!(matches!(
        authorize_loopback_request(
            &bootstrap,
            &format!("{PLUGIN_SESSION_BOOTSTRAP_PATH}?token={token}"),
            &token,
            "/api/health",
        ),
        Some(LoopbackRequestAuthorization::Bootstrap)
    ));
    let wrong_bootstrap = format!(
        "GET {PLUGIN_SESSION_BOOTSTRAP_PATH}?token={} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        "z".repeat(43)
    );
    assert!(authorize_loopback_request(
        &wrong_bootstrap,
        &format!("{PLUGIN_SESSION_BOOTSTRAP_PATH}?token={}", "z".repeat(43)),
        &token,
        "/api/health",
    )
    .is_none());
    let session = format!("GET / HTTP/1.1\r\nCookie: {PLUGIN_SESSION_COOKIE}={token}\r\n\r\n");
    assert!(matches!(
        authorize_loopback_request(&session, "/", &token, "/api/health"),
        Some(LoopbackRequestAuthorization::Session)
    ));
    let duplicate_session = format!(
        "GET / HTTP/1.1\r\nCookie: {PLUGIN_SESSION_COOKIE}={token}; {PLUGIN_SESSION_COOKIE}={token}\r\n\r\n"
    );
    assert!(authorize_loopback_request(&duplicate_session, "/", &token, "/api/health").is_none());
    let duplicate_cookie_headers = format!(
        "GET / HTTP/1.1\r\nCookie: {PLUGIN_SESSION_COOKIE}={token}\r\nCookie: unrelated=value\r\n\r\n"
    );
    assert!(
        authorize_loopback_request(&duplicate_cookie_headers, "/", &token, "/api/health").is_none()
    );
    assert!(authorize_loopback_request(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        "/",
        &token,
        "/api/health",
    )
    .is_none());
    let health = format!(
        "GET /api/health HTTP/1.1\r\n{PLUGIN_HEALTH_CHALLENGE_HEADER}: {}\r\n\r\n",
        "b".repeat(43)
    );
    assert!(matches!(
        authorize_loopback_request(&health, "/api/health", &token, "/api/health"),
        Some(LoopbackRequestAuthorization::Health(_))
    ));
    let challenge = "b".repeat(43);
    let proof = URL_SAFE_NO_PAD
        .decode(health_proof(&token, &challenge))
        .unwrap();
    assert!(hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA256, token.as_bytes()),
        challenge.as_bytes(),
        &proof,
    )
    .is_ok());
    assert!(hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA256, token.as_bytes()),
        "c".repeat(43).as_bytes(),
        &proof,
    )
    .is_err());
    let injected = inject_authentication_header(session.as_bytes(), &token).unwrap();
    assert!(std::str::from_utf8(&injected)
        .unwrap()
        .contains(&format!("{PLUGIN_AUTHENTICATION_HEADER}: {token}\r\n")));
    assert!(inject_authentication_header(
        format!("GET / HTTP/1.1\r\n{PLUGIN_AUTHENTICATION_HEADER}: attacker\r\n\r\n").as_bytes(),
        &token,
    )
    .is_none());
}
