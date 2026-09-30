use super::*;
use serde_json::json;

fn request(request_id: &str) -> RelayRequest {
    RelayRequest {
        message_type: "companion_resources_request".to_string(),
        request_id: request_id.to_string(),
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        workspace_id: String::new(),
        method: "GET".to_string(),
        path: "/companion/resources".to_string(),
        headers: BTreeMap::new(),
        body: Value::Null,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    }
}

async fn connected_relay() -> (ConnectorRelay, mpsc::Receiver<String>) {
    let relay = ConnectorRelay::default();
    let (outbound, receiver) = mpsc::channel(8);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;
    (relay, receiver)
}

#[tokio::test]
async fn active_session_requires_matching_owner_and_registered_websocket() {
    let (relay, _) = connected_relay().await;
    assert!(relay
        .has_active_session("owner-1", "device-1")
        .await
        .unwrap());
    assert!(!relay
        .has_active_session("owner-2", "device-1")
        .await
        .unwrap());
    relay.unregister_session("device-1", "session-1").await;
    assert!(!relay
        .has_active_session("owner-1", "device-1")
        .await
        .unwrap());
}

#[tokio::test]
async fn companion_response_completes_the_original_request() {
    let (relay, mut outbound) = connected_relay().await;
    let dispatched = {
        let relay = relay.clone();
        tokio::spawn(async move {
            relay
                .dispatch_companion(request("request-1"), Duration::from_secs(2), "client-1")
                .await
        })
    };
    let sent = outbound.recv().await.expect("dispatched request");
    assert_eq!(
        serde_json::from_str::<Value>(sent.as_str()).unwrap()["type"],
        "companion_resources_request"
    );
    assert!(relay
        .handle_inbound_text_from(
            RelaySessionIdentity {
                owner_user_id: "owner-1".to_string(),
                device_id: "device-1".to_string(),
                session_id: "session-1".to_string(),
            },
            &json!({
                "type": "companion_resources_response",
                "request_id": "request-1",
                "status": 200,
                "body": {"resources": []}
            })
            .to_string(),
        )
        .await
        .unwrap());
    let response = dispatched.await.unwrap().unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, json!({"resources": []}));
}

#[tokio::test]
async fn response_from_another_connector_session_is_rejected() {
    let (relay, mut outbound) = connected_relay().await;
    let dispatched = {
        let relay = relay.clone();
        tokio::spawn(async move {
            relay
                .dispatch_companion(request("request-2"), Duration::from_millis(100), "client-1")
                .await
        })
    };
    outbound.recv().await.expect("dispatched request");
    let error = relay
        .handle_inbound_text_from(
            RelaySessionIdentity {
                owner_user_id: "owner-1".to_string(),
                device_id: "device-1".to_string(),
                session_id: "other-session".to_string(),
            },
            &json!({
                "type": "companion_resources_response",
                "request_id": "request-2",
                "status": 200
            })
            .to_string(),
        )
        .await
        .expect_err("wrong source must fail");
    assert!(error.contains("source does not match"));
    assert!(matches!(
        dispatched.await.unwrap(),
        Err(RelayError::Timeout)
    ));
}

#[tokio::test]
async fn removed_execution_response_types_are_rejected() {
    let relay = ConnectorRelay::default();
    let error = relay
        .handle_inbound_text(
            &json!({
                "type": "terminal_response",
                "request_id": "legacy-request",
                "status": 200
            })
            .to_string(),
        )
        .await
        .expect_err("legacy execution response must fail closed");
    assert!(error.contains("unsupported relay response type"));
}

#[tokio::test]
async fn companion_pending_limits_are_scoped_by_device_and_client() {
    let relay = ConnectorRelay::default();
    let source = RelaySessionIdentity {
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        session_id: "session-1".to_string(),
    };
    for index in 0..MAX_PENDING_COMPANION_REQUESTS_PER_CLIENT_SESSION {
        relay
            .insert_pending_request(
                format!("request-{index}").as_str(),
                source.clone(),
                PendingRelayClass::Companion {
                    client_session_id: "client-1".to_string(),
                },
                Instant::now() + Duration::from_secs(10),
            )
            .await
            .unwrap();
    }
    let error = relay
        .insert_pending_request(
            "request-over-limit",
            source,
            PendingRelayClass::Companion {
                client_session_id: "client-1".to_string(),
            },
            Instant::now() + Duration::from_secs(10),
        )
        .await
        .expect_err("per-client limit must reject");
    assert!(matches!(error, RelayError::TooManyPendingRequests { .. }));
}

#[tokio::test]
async fn pending_reaper_removes_expired_requests() {
    let relay = ConnectorRelay::default();
    relay
        .insert_pending_request(
            "expired",
            RelaySessionIdentity {
                owner_user_id: "owner-1".to_string(),
                device_id: "device-1".to_string(),
                session_id: "session-1".to_string(),
            },
            PendingRelayClass::Companion {
                client_session_id: "client-1".to_string(),
            },
            Instant::now() - Duration::from_secs(1),
        )
        .await
        .unwrap();
    assert_eq!(relay.reap_expired_pending().await, 1);
    assert_eq!(relay.stats().await.pending_relay_requests, 0);
}

#[test]
fn inter_instance_protocol_contains_only_dispatch_and_response_messages() {
    let dispatch = InterInstanceRelayMessage::Dispatch {
        request: request("request-3"),
        requester_instance_id: "instance-a".to_string(),
    };
    assert_eq!(serde_json::to_value(dispatch).unwrap()["type"], "dispatch");
    let response = InterInstanceRelayMessage::Response {
        response: RelayResponse {
            request_id: "request-3".to_string(),
            status: 200,
            headers: BTreeMap::new(),
            body: Value::Null,
        },
    };
    assert_eq!(serde_json::to_value(response).unwrap()["type"], "response");
}
