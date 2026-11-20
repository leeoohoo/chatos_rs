use super::*;
use crate::plugin_application::{
    load_manifest, project_application, PathReference, PluginRuntimeContext,
};
use chatos_local_agent_protocol::{LocalPluginInstallationRecord, LocalPluginInstallationSpec};
use serde_json::json;
use std::io::Write as _;

struct RuntimeFixture {
    _directory: tempfile::TempDir,
    app_data: PathBuf,
    installation: PathBuf,
    resolved: ResolvedPluginApplication,
}

impl RuntimeFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let connector = app_data.join("NativeConnector");
        let installation = connector.join("Plugins").join("plugin-1").join("1.0.0");
        fs::create_dir_all(installation.join("ui")).unwrap();
        fs::write(installation.join("ui/index.html"), "<main>Plugin</main>").unwrap();
        let fixture_name = if cfg!(windows) {
            "plugin-runtime-fixture.exe"
        } else {
            "plugin-runtime-fixture"
        };
        let fixture_executable = installation.join(fixture_name);
        fs::copy(std::env::current_exe().unwrap(), &fixture_executable).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fixture_executable.metadata().unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&fixture_executable, permissions).unwrap();
        }
        fs::write(
            installation.join("package.json"),
            serde_json::to_vec(&json!({
                "name": "plugin-runtime-fixture",
                "bin": {"fixture": fixture_name}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            installation.join("chatos.plugin.json"),
            serde_json::to_vec(&json!({
                "schemaVersion": 3,
                "name": "plugin-runtime-fixture",
                "version": "1.0.0",
                "description": "Runtime fixture",
                "ui": [{
                    "componentKey": "plugin-workbench",
                    "source": "./ui/index.html",
                    "title": "Plugin Runtime",
                    "surface": "workbench",
                    "bridgeCapabilities": ["host.context.read", "artifact.read"],
                    "runtime": {
                        "type": "local_http",
                        "bin": "fixture",
                        "args": [
                            "--exact",
                            "plugin_application::runtime::tests::plugin_runtime_fixture_process",
                            "--nocapture"
                        ],
                        "healthPath": "/api/health",
                        "launchTimeoutMs": 15000
                    }
                }],
                "dependencies": {"supportedPlatforms": ["windows"]},
                "permissions": [{
                    "permission": "process.spawn",
                    "required": true,
                    "components": ["plugin-workbench"]
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            connector.join("state.json"),
            serde_json::to_vec(&json!({
                "user": {"id": "owner-1"},
                "deviceID": "device-1",
                "workspaces": []
            }))
            .unwrap(),
        )
        .unwrap();
        let record = LocalPluginInstallationRecord {
            spec: LocalPluginInstallationSpec {
                installation_id: "installation-1".into(),
                owner_user_id: "owner-1".into(),
                plugin_id: "plugin-1".into(),
                release_id: "release-1".into(),
                release_digest: format!("sha256:{}", "a".repeat(64)),
                component_id: "component-1".into(),
                component_revision: "revision-1".into(),
                server_id: "server-1".into(),
                executable_path: fixture_executable.to_string_lossy().into_owned(),
                args: Vec::new(),
                working_directory: None,
                environment_secret_refs: BTreeMap::from([(
                    "API_TOKEN".into(),
                    "secret-reference".into(),
                )]),
                tool_prefix: None,
                allowed_tools: None,
                enabled: true,
            },
            version: 1,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
        };
        let plugin_root = connector.join("Plugins").canonicalize().unwrap();
        let (resolved_installation, manifest) = load_manifest(&plugin_root, &record).unwrap();
        let contribution = manifest.ui[0].clone();
        let application = project_application(
            &plugin_root,
            &resolved_installation,
            &record,
            &manifest,
            &contribution,
            None,
            &mut BTreeMap::new(),
        )
        .unwrap();
        Self {
            _directory: directory,
            app_data,
            installation,
            resolved: ResolvedPluginApplication {
                record,
                installation_root: resolved_installation,
                manifest,
                contribution,
                application,
            },
        }
    }

    fn runtime(&self) -> PluginApplicationRuntime {
        PluginApplicationRuntime::new(self.app_data.join("NativeConnector").join("PluginRuntime"))
    }

    fn request(&self, generation: u64) -> PluginApplicationLaunchRequest {
        PluginApplicationLaunchRequest {
            owner_user_id: "owner-1".into(),
            presentation_id: "presentation-1".into(),
            generation,
            plugin_id: "plugin-1".into(),
            component_key: "plugin-workbench".into(),
            context: PluginApplicationContextSelection::None,
        }
    }
}

#[test]
fn plugin_runtime_fixture_process() {
    let Ok(port) = std::env::var("CHATOS_PLUGIN_APP_PORT") else {
        return;
    };
    let authentication_token = std::env::var("CHATOS_PLUGIN_APP_TOKEN").unwrap();
    let health_path = std::env::var("CHATOS_PLUGIN_APP_HEALTH_PATH").unwrap();
    let listener = std::net::TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let mut request = [0_u8; 8 * 1_024];
        let count = stream.read(&mut request).unwrap_or(0);
        let request = std::str::from_utf8(&request[..count]).unwrap_or_default();
        let authenticated = unique_header_value(request, PLUGIN_AUTHENTICATION_HEADER)
            .is_some_and(|value| secure_equal(value, &authentication_token));
        let health = request.starts_with(&format!("GET {health_path} "));
        let application = request.starts_with("GET / ");
        let success = authenticated && (health || application);
        let challenge = unique_header_value(request, PLUGIN_HEALTH_CHALLENGE_HEADER);
        let (status, body) = if success {
            ("200 OK", "ok")
        } else {
            ("404 Not Found", "missing")
        };
        let proof = challenge
            .filter(|_| authenticated && health)
            .map(|challenge| {
                format!(
                    "{PLUGIN_HEALTH_PROOF_HEADER}: {}\r\n",
                    health_proof(&authentication_token, challenge)
                )
            })
            .unwrap_or_default();
        let response = format!(
            "HTTP/1.1 {status}\r\n{proof}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    }
}

#[tokio::test]
async fn runtime_launches_reuses_and_resets_a_healthy_owned_process() {
    let fixture = RuntimeFixture::new();
    let runtime = fixture.runtime();
    let epoch = runtime.epoch();

    let first = runtime
        .launch(epoch, fixture.request(1), fixture.resolved.clone())
        .await
        .unwrap();
    let second = runtime
        .launch(epoch, fixture.request(2), fixture.resolved.clone())
        .await
        .unwrap();

    assert_eq!(first.url, second.url);
    assert_ne!(first.session_id, second.session_id);
    assert_eq!(runtime.resources.used_resource_units(), 1);
    assert!(first.url.starts_with("http://127.0.0.1:"));
    assert!(first.url.contains(PLUGIN_SESSION_BOOTSTRAP_PATH));
    assert_eq!(second.bridge_capabilities, ["host.context.read"]);
    assert_eq!(second.host_session_nonce.len(), 64);
    assert!(second
        .application
        .bridge_capabilities
        .contains(&"artifact.read".to_owned()));
    assert_eq!(runtime.state.lock().await.processes.len(), 1);
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(!serialized.contains(&fixture.app_data.to_string_lossy().to_string()));
    assert!(!serialized.contains("API_TOKEN"));
    assert!(!serialized.contains("secret-reference"));

    let client = Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(format!("{}/", first.bridge_origin))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let bootstrap = client.get(&first.url).send().await.unwrap();
    assert_eq!(bootstrap.status(), reqwest::StatusCode::SEE_OTHER);
    assert_eq!(bootstrap.headers()["location"], "/");
    assert_eq!(bootstrap.headers()["referrer-policy"], "no-referrer");
    let set_cookie = bootstrap.headers()["set-cookie"].to_str().unwrap();
    assert!(set_cookie.contains("; HttpOnly;"));
    assert!(set_cookie.contains("; SameSite=Strict;"));
    assert!(set_cookie.ends_with("; Path=/"));
    let cookie = set_cookie.split(';').next().unwrap();
    assert!(client
        .get(format!("{}/", first.bridge_origin))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap()
        .status()
        .is_success());

    let bridge_request = PluginApplicationBridgeRequest {
        owner_user_id: "owner-1".into(),
        presentation_id: "presentation-1".into(),
        generation: 2,
        session_id: second.session_id.clone(),
        host_session_nonce: second.host_session_nonce.clone(),
        origin: second.bridge_origin.clone(),
        request_id: "request-1".into(),
        method: "host.context.read".into(),
        payload: json!({}),
    };
    let bridge_response = runtime
        .bridge_request(bridge_request.clone())
        .await
        .unwrap();
    assert_eq!(bridge_response.request_id, "request-1");
    assert_eq!(
        bridge_response.result["capabilities"],
        json!(["host.context.read"])
    );
    assert!(bridge_response.result["projectId"].is_null());
    assert!(bridge_response.result["projectName"].is_null());

    let mut wrong_origin = bridge_request.clone();
    wrong_origin.origin = "http://127.0.0.1:1".into();
    assert!(runtime.bridge_request(wrong_origin).await.is_err());
    let mut wrong_nonce = bridge_request.clone();
    wrong_nonce.host_session_nonce = "0".repeat(64);
    assert!(runtime.bridge_request(wrong_nonce).await.is_err());
    let mut undeclared = bridge_request.clone();
    undeclared.method = "artifact.read".into();
    assert!(runtime.bridge_request(undeclared).await.is_err());

    runtime
        .discard_presentation("owner-1", "presentation-1", 2)
        .await
        .unwrap();
    assert!(runtime.bridge_request(bridge_request).await.is_err());
    assert!(runtime
        .launch(epoch, fixture.request(2), fixture.resolved.clone())
        .await
        .unwrap_err()
        .contains("失效"));
    runtime.reset().await;
    assert_ne!(runtime.epoch(), epoch);
    assert!(runtime.state.lock().await.processes.is_empty());
    assert_eq!(runtime.resources.used_resource_units(), 0);
    let stop_client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();
    let mut stopped = false;
    for _ in 0..20 {
        if stop_client.get(&first.bridge_origin).send().await.is_err() {
            stopped = true;
            break;
        }
        sleep(Duration::from_millis(25)).await;
    }
    assert!(stopped);
}

#[tokio::test]
async fn static_application_serves_only_declared_bounded_assets_and_keeps_bridge_binding() {
    let mut fixture = RuntimeFixture::new();
    fs::write(
        fixture.installation.join("ui/index.html"),
        "<script type=\"module\" src=\"./app.js\"></script><main>Static Plugin</main>",
    )
    .unwrap();
    fs::write(
        fixture.installation.join("ui/app.js"),
        "window.staticReady = true;",
    )
    .unwrap();
    fs::write(fixture.installation.join("ui/private.txt"), "not declared").unwrap();
    fixture.resolved.contribution.runtime = None;
    fixture.resolved.contribution.assets = vec![PathReference::Text("./ui/app.js".into())];
    fixture.resolved.application.requires_local_runtime = false;
    let runtime = fixture.runtime();
    let launch = runtime
        .launch(
            runtime.epoch(),
            fixture.request(1),
            fixture.resolved.clone(),
        )
        .await
        .unwrap();

    let client = Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(format!("{}/", launch.bridge_origin))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let bootstrap = client.get(&launch.url).send().await.unwrap();
    assert_eq!(bootstrap.status(), reqwest::StatusCode::SEE_OTHER);
    assert_eq!(bootstrap.headers()["location"], "/");
    let cookie = bootstrap.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert!(!cookie.is_empty());
    let entry = client
        .get(format!("{}/", launch.bridge_origin))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert!(entry.status().is_success());
    assert_eq!(
        entry.headers()["x-content-type-options"].to_str().unwrap(),
        "nosniff"
    );
    assert!(entry.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("connect-src 'none'"));
    assert!(entry.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("navigate-to 'self'"));
    assert!(entry.text().await.unwrap().contains("Static Plugin"));

    let script = client
        .get(format!("{}/app.js", launch.bridge_origin))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert!(script.status().is_success());
    assert_eq!(
        script.headers()["content-type"].to_str().unwrap(),
        "text/javascript; charset=utf-8"
    );
    assert!(script.text().await.unwrap().contains("staticReady"));
    assert_eq!(
        client
            .get(format!("{}/private.txt", launch.bridge_origin))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::NOT_FOUND
    );

    let response = runtime
        .bridge_request(PluginApplicationBridgeRequest {
            owner_user_id: "owner-1".into(),
            presentation_id: "presentation-1".into(),
            generation: 1,
            session_id: launch.session_id,
            host_session_nonce: launch.host_session_nonce,
            origin: launch.bridge_origin,
            request_id: "static-context".into(),
            method: "host.context.read".into(),
            payload: json!({}),
        })
        .await
        .unwrap();
    assert_eq!(response.request_id, "static-context");
    runtime.reset().await;
    assert!(runtime.state.lock().await.processes.is_empty());
}

#[tokio::test]
async fn authenticated_loopback_proxy_preserves_an_upgraded_bidirectional_stream() {
    let authentication_token = "a".repeat(43);
    let target_listener =
        TokioTcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
    let target_port = target_listener.local_addr().unwrap().port();
    let expected_token = authentication_token.clone();
    let target = tokio::spawn(async move {
        let (stream, _) = target_listener.accept().await.unwrap();
        let mut stream = BufStream::new(stream);
        let headers = read_proxy_headers(&mut stream).await.unwrap();
        let request = std::str::from_utf8(&headers).unwrap();
        assert_eq!(
            unique_header_value(request, PLUGIN_AUTHENTICATION_HEADER),
            Some(expected_token.as_str())
        );
        stream
            .write_all(
                b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: chatos-test\r\n\r\n",
            )
            .await
            .unwrap();
        stream.flush().await.unwrap();
        let mut ping = [0_u8; 4];
        stream.read_exact(&mut ping).await.unwrap();
        assert_eq!(&ping, b"PING");
        stream.write_all(b"PONG").await.unwrap();
        stream.flush().await.unwrap();
    });

    let proxy_port = available_loopback_port().unwrap();
    let proxy = start_loopback_proxy(
        proxy_port,
        target_port,
        "/api/health".into(),
        authentication_token.clone(),
    )
    .await
    .unwrap();
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy_port))
        .await
        .unwrap();
    let mut stream = BufStream::new(stream);
    stream
        .write_all(
            format!(
                "GET /socket HTTP/1.1\r\nHost: 127.0.0.1:{proxy_port}\r\nCookie: {PLUGIN_SESSION_COOKIE}={authentication_token}\r\nConnection: Upgrade\r\nUpgrade: chatos-test\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    stream.flush().await.unwrap();
    let response = read_proxy_headers(&mut stream).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 101 Switching Protocols\r\n"));
    stream.write_all(b"PING").await.unwrap();
    stream.flush().await.unwrap();
    let mut pong = [0_u8; 4];
    stream.read_exact(&mut pong).await.unwrap();
    assert_eq!(&pong, b"PONG");

    target.await.unwrap();
    proxy.request();
}

#[tokio::test]
async fn reset_invalidates_an_in_flight_presentation_epoch() {
    let fixture = RuntimeFixture::new();
    let runtime = fixture.runtime();
    let epoch = runtime.epoch();
    let request = fixture.request(1);
    runtime.begin_presentation(epoch, &request).await.unwrap();

    runtime.reset().await;

    assert!(!runtime.accepts(epoch, &request).await);
    assert!(runtime
        .begin_presentation(epoch, &request)
        .await
        .unwrap_err()
        .contains("失效"));
}

#[tokio::test]
async fn failed_health_start_releases_the_process_and_capacity_reservation() {
    let mut fixture = RuntimeFixture::new();
    fixture.resolved.contribution.runtime.as_mut().unwrap().args = vec![
        "--exact".into(),
        "plugin_application::runtime::tests::missing_fixture".into(),
        "--nocapture".into(),
    ];
    let runtime = fixture.runtime();

    let error = runtime
        .launch(
            runtime.epoch(),
            fixture.request(1),
            fixture.resolved.clone(),
        )
        .await
        .unwrap_err();

    assert!(error.contains("退出") || error.contains("超时"));
    let state = runtime.state.lock().await;
    assert!(state.processes.is_empty());
    assert!(state.reservations.is_empty());
}

#[path = "tests/validation.rs"]
mod validation;
