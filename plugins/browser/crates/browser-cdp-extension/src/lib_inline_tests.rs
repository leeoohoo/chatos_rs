#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use async_tungstenite::{
        tokio::accept_hdr_async,
        tungstenite::{
            Message,
            handshake::server::{Request, Response},
            http::HeaderValue,
        },
    };
    use browser_cdp_core::BrowserBackend;
    use futures::StreamExt;
    use tempfile::tempdir;
    use tokio::net::TcpListener;

    use super::*;

    const TEST_TOKEN: &str = "test-bridge-token-0123456789";

    #[test]
    fn endpoint_is_strictly_loopback_and_carries_no_credentials() {
        assert!(validate_endpoint("ws://127.0.0.1:9223/v1/browser").is_ok());
        assert!(validate_endpoint("ws://[::1]:9223/v1/browser").is_ok());
        assert!(validate_endpoint("ws://localhost:9223/v1/browser").is_err());
        assert!(validate_endpoint("ws://192.0.2.1:9223/v1/browser").is_err());
        assert!(validate_endpoint("wss://127.0.0.1:9223/v1/browser").is_err());
        assert!(validate_endpoint("ws://user:secret@127.0.0.1:9223/").is_err());
        assert!(validate_endpoint("ws://127.0.0.1:9223/?token=secret").is_err());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn credential_file_must_be_private_and_unexpired() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("bridge.json");
        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 60_000;
        tokio::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "token": TEST_TOKEN,
                "expires_at_unix_ms": expires
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .unwrap();
        assert_eq!(read_credential_file(&path).await.unwrap(), TEST_TOKEN);
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .await
            .unwrap();
        assert!(read_credential_file(&path).await.is_err());
    }

    #[tokio::test]
    async fn bridge_backend_authenticates_routes_commands_and_events() {
        let (endpoint, server) = spawn_mock_bridge(TEST_TOKEN, false, false).await;
        let config = BridgeConfig::new(&endpoint, TEST_TOKEN.into()).unwrap();
        let factory = ExtensionBackendFactory::with_config(config);
        let backend = factory.create(BrowserMode::ChromeExtension).await.unwrap();
        let descriptor = backend
            .open(OpenBrowserRequest {
                mode: BrowserMode::ChromeExtension,
                headless: true,
                persistent_profile: false,
                session_name: Some("Release verification".into()),
            })
            .await
            .unwrap();
        assert_eq!(descriptor.product, "Mock Chrome/1");
        assert!(descriptor.capabilities.contains(&"existing_chrome".into()));

        let targets = backend.list_targets().await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "approved-tab-1");
        let session = backend.attach_target(&targets[0].id).await.unwrap();
        let value = backend
            .send_command(
                Some(&session),
                "Runtime.evaluate",
                json!({ "expression": "1 + 1" }),
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert_eq!(value["result"]["value"], 2);

        let subscription_id = backend
            .subscribe(EventFilter {
                methods: vec!["Runtime.consoleAPICalled".into()],
                session_id: Some(session.clone()),
            })
            .await
            .unwrap();
        let events = backend
            .poll_events(&subscription_id, 0, 10, Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(events.events.len(), 1);
        assert_eq!(events.events[0].method, "Runtime.consoleAPICalled");
        assert_eq!(events.events[0].params["authorization"], "[REDACTED]");

        let unsupported = backend
            .send_command(
                None,
                "Browser.unsupported",
                json!({}),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        assert!(matches!(unsupported, CoreError::Unsupported(_)));
        backend.unsubscribe(&subscription_id).await.unwrap();
        backend.detach_target(&session).await.unwrap();
        backend.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn authentication_failure_is_generic_and_fails_closed() {
        let (endpoint, server) = spawn_mock_bridge(TEST_TOKEN, true, false).await;
        let config = BridgeConfig::new(&endpoint, "wrong-bridge-token-012345".into()).unwrap();
        let backend = ExtensionCdpBackend::new(config);
        let error = backend
            .open(OpenBrowserRequest {
                mode: BrowserMode::ChromeExtension,
                headless: true,
                persistent_profile: false,
                session_name: None,
            })
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "browser backend error: Browser Bridge authentication failed"
        );
        assert!(!error.to_string().contains(TEST_TOKEN));
        server.await.unwrap();
    }

    #[test]
    fn disconnected_extension_authentication_error_is_actionable() {
        let result = parse_authentication_response(Message::text(
            json!({
                "type": "response",
                "id": 1,
                "error": {
                    "code": "extension_unavailable",
                    "message": "Chrome extension is not connected"
                }
            })
            .to_string(),
        ));
        let error = match result {
            Ok(_) => panic!("disconnected extension authentication must fail"),
            Err(error) => error,
        };
        assert_eq!(
            error.to_string(),
            "browser backend error: Chrome extension is not connected. Start a Browser CDP task, then click First connect in the Chatos Browser Bridge extension."
        );
    }

    #[tokio::test]
    async fn extension_disconnect_immediately_invalidates_event_polling() {
        let (endpoint, server) = spawn_mock_bridge(TEST_TOKEN, false, true).await;
        let config = BridgeConfig::new(&endpoint, TEST_TOKEN.into()).unwrap();
        let backend = ExtensionCdpBackend::new(config);
        backend
            .open(OpenBrowserRequest {
                mode: BrowserMode::ChromeExtension,
                headless: true,
                persistent_profile: false,
                session_name: None,
            })
            .await
            .unwrap();
        let subscription_id = backend
            .subscribe(EventFilter {
                methods: vec!["Browser.downloadWillBegin".into()],
                session_id: None,
            })
            .await
            .unwrap();
        let error = backend
            .poll_events(&subscription_id, 0, 10, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("extension_disabled"));
        backend.close().await.unwrap();
        server.await.unwrap();
    }

    #[allow(clippy::result_large_err)] // Tungstenite fixes the callback error response type.
    async fn spawn_mock_bridge(
        expected_token: &'static str,
        reject_auth: bool,
        disconnect_after_subscribe: bool,
    ) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket =
                accept_hdr_async(stream, |request: &Request, mut response: Response| {
                    let protocol = request.headers().get(SEC_WEBSOCKET_PROTOCOL).unwrap();
                    assert_eq!(protocol, BRIDGE_SUBPROTOCOL);
                    response.headers_mut().insert(
                        SEC_WEBSOCKET_PROTOCOL,
                        HeaderValue::from_static(BRIDGE_SUBPROTOCOL),
                    );
                    Ok(response)
                })
                .await
                .unwrap();
            let authentication = socket.next().await.unwrap().unwrap();
            let Message::Text(authentication) = authentication else {
                panic!("authentication must be text");
            };
            let authentication: Value = serde_json::from_str(&authentication).unwrap();
            assert_eq!(authentication["method"], "bridge.authenticate");
            if reject_auth || authentication["params"]["token"] != expected_token {
                socket
                    .send(Message::text(
                        json!({
                            "type": "response",
                            "id": 1,
                            "error": {"code": "token_expired", "message": "secret details"}
                        })
                        .to_string(),
                    ))
                    .await
                    .unwrap();
                let _ = socket.close(None).await;
                return;
            }
            socket
                .send(Message::text(
                    json!({
                        "type": "response",
                        "id": 1,
                        "result": {
                            "protocol_version": BRIDGE_PROTOCOL_VERSION,
                            "connection_id": "mock-connection",
                            "product": "Mock Chrome/1",
                            "user_agent": "Mock Chrome",
                            "capabilities": ["page_control", "raw_cdp", "native_tab_groups"]
                        }
                    })
                    .to_string(),
                ))
                .await
                .unwrap();
            while let Some(Ok(Message::Text(text))) = socket.next().await {
                let request: Value = serde_json::from_str(&text).unwrap();
                let id = request["id"].as_u64().unwrap();
                let method = request["method"].as_str().unwrap();
                let result = match method {
                    "bridge.configureSession" => {
                        assert!(
                            request["params"]["session_name"]
                                .as_str()
                                .is_some_and(|value| !value.trim().is_empty())
                        );
                        json!({})
                    }
                    "bridge.listTargets" => json!({
                        "targets": [{
                            "id": "approved-tab-1",
                            "title": "Approved",
                            "url": "https://example.test/",
                            "kind": "page"
                        }]
                    }),
                    "bridge.attachTarget" => json!({"session_id": "remote-session-1"}),
                    "bridge.detachTarget" | "bridge.unsubscribe" | "bridge.close" => json!({}),
                    "cdp.send" if request["params"]["method"] == "Browser.unsupported" => {
                        socket
                            .send(Message::text(
                                json!({
                                    "type": "response",
                                    "id": id,
                                    "error": {
                                        "code": "unsupported_by_backend",
                                        "message": "browser command is unavailable"
                                    }
                                })
                                .to_string(),
                            ))
                            .await
                            .unwrap();
                        continue;
                    }
                    "cdp.send" => json!({
                        "result": {"result": {"type": "number", "value": 2}}
                    }),
                    "bridge.subscribe" => {
                        let subscription_id = request["params"]["subscription_id"].clone();
                        socket
                            .send(Message::text(
                                json!({"type": "response", "id": id, "result": {}}).to_string(),
                            ))
                            .await
                            .unwrap();
                        if disconnect_after_subscribe {
                            socket
                                .send(Message::text(
                                    json!({
                                        "type": "event",
                                        "method": "bridge.disconnected",
                                        "params": {"reason": "extension_disabled"}
                                    })
                                    .to_string(),
                                ))
                                .await
                                .unwrap();
                            let _ = socket.close(None).await;
                            break;
                        }
                        socket
                            .send(Message::text(
                                json!({
                                    "type": "event",
                                    "method": "cdp.event",
                                    "params": {
                                        "subscription_id": subscription_id,
                                        "session_id": "remote-session-1",
                                        "method": "Runtime.consoleAPICalled",
                                        "params": {"authorization": "Bearer secret"}
                                    }
                                })
                                .to_string(),
                            ))
                            .await
                            .unwrap();
                        continue;
                    }
                    other => panic!("unexpected Bridge method {other}"),
                };
                socket
                    .send(Message::text(
                        json!({"type": "response", "id": id, "result": result}).to_string(),
                    ))
                    .await
                    .unwrap();
                if method == "bridge.close" {
                    let _ = socket.close(None).await;
                    break;
                }
            }
        });
        (format!("ws://{address}/v1/browser"), task)
    }
}
