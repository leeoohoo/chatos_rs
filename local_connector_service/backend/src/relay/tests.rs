use std::collections::BTreeMap;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::*;
use crate::managed_config::RelayRuntimeLimits;
use crate::pressure::PlatformPressureLevel;
use crate::valkey_coordination::DevicePresence;

fn relay_request(request_id: &str) -> RelayRequest {
    RelayRequest {
        message_type: "plugin_prepare_request".to_string(),
        request_id: request_id.to_string(),
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        method: "POST".to_string(),
        path: "/plugins/prepare".to_string(),
        headers: BTreeMap::new(),
        body: serde_json::json!({"plugin_id":"plugin-browser"}),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    }
}

#[tokio::test]
async fn active_session_requires_matching_owner_and_registered_websocket() {
    let relay = ConnectorRelay::default();
    assert!(!relay
        .has_active_session("owner-1", "device-1")
        .await
        .expect("query empty relay"));

    let (outbound, _inbound) = mpsc::channel(1);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;

    assert!(relay
        .has_active_session("owner-1", "device-1")
        .await
        .expect("query registered relay"));
    assert!(!relay
        .has_active_session("owner-2", "device-1")
        .await
        .expect("query mismatched owner"));

    relay.unregister_session("device-1", "session-1").await;
    assert!(!relay
        .has_active_session("owner-1", "device-1")
        .await
        .expect("query unregistered relay"));
}

async fn spawn_test_instance_listener(
    coordinator: ValkeyCoordinator,
    instance_id: String,
    relay: ConnectorRelay,
) -> tokio::task::JoinHandle<()> {
    let mut pubsub = coordinator
        .subscribe_instance(instance_id.as_str())
        .await
        .expect("subscribe test relay instance");
    tokio::spawn(async move {
        let mut messages = pubsub.on_message();
        while let Some(message) = messages.next().await {
            let payload = message
                .get_payload::<String>()
                .expect("decode test relay message");
            let message = serde_json::from_str::<InterInstanceRelayMessage>(&payload)
                .expect("parse test relay message");
            relay
                .handle_inter_instance_message(message)
                .await
                .expect("handle test relay message");
        }
    })
}

#[test]
fn inter_instance_dispatch_message_has_versioned_tagged_shape() {
    let message = InterInstanceRelayMessage::Dispatch {
        request: relay_request("request-1"),
        requester_instance_id: "local-connector-requester".to_string(),
    };

    let value = serde_json::to_value(&message).expect("serialize dispatch message");
    assert_eq!(value["type"], "dispatch");
    assert_eq!(value["requester_instance_id"], "local-connector-requester");
    assert_eq!(value["request"]["request_id"], "request-1");
    let decoded: InterInstanceRelayMessage =
        serde_json::from_value(value).expect("deserialize dispatch message");
    assert!(matches!(
        decoded,
        InterInstanceRelayMessage::Dispatch { .. }
    ));
}

#[test]
fn inter_instance_send_message_keeps_delivery_ack_route() {
    let message = InterInstanceRelayMessage::Send {
        request: relay_request("request-2"),
        requester_instance_id: "local-connector-requester".to_string(),
    };

    let value = serde_json::to_value(&message).expect("serialize send message");
    assert_eq!(value["type"], "send");
    assert_eq!(value["requester_instance_id"], "local-connector-requester");
    assert_eq!(value["request"]["request_id"], "request-2");
}

#[test]
fn inter_instance_response_supports_delivery_ack_shape() {
    let message = InterInstanceRelayMessage::Response {
        response: RelayResponse {
            request_id: "request-2".to_string(),
            status: 202,
            headers: BTreeMap::new(),
            body: serde_json::json!({"delivered": true}),
        },
    };

    let value = serde_json::to_value(&message).expect("serialize response message");
    assert_eq!(value["type"], "response");
    assert_eq!(value["response"]["request_id"], "request-2");
    assert_eq!(value["response"]["status"], 202);
    assert_eq!(value["response"]["body"]["delivered"], true);
}

#[test]
fn inter_instance_terminal_event_contains_ephemeral_body_only() {
    let message = InterInstanceRelayMessage::TerminalEvent {
        event: TerminalRelayEvent {
            message_type: "terminal_output".to_string(),
            terminal_session_id: "terminal-1".to_string(),
            body: serde_json::json!({"data": "hello"}),
        },
    };

    let value = serde_json::to_value(&message).expect("serialize terminal event");
    assert_eq!(value["type"], "terminal_event");
    assert_eq!(value["event"]["terminal_session_id"], "terminal-1");
    assert_eq!(value["event"]["body"]["data"], "hello");
    assert!(value.get("queue_name").is_none());
}

#[tokio::test]
#[ignore = "requires CHATOS_LOCAL_CONNECTOR_TEST_VALKEY_URL"]
async fn real_valkey_routes_commands_responses_and_terminal_events_across_instances() {
    let valkey_url = std::env::var("CHATOS_LOCAL_CONNECTOR_TEST_VALKEY_URL")
        .expect("CHATOS_LOCAL_CONNECTOR_TEST_VALKEY_URL must be set");
    let test_id = Uuid::new_v4().to_string();
    let key_prefix = format!("test:local-connector:{test_id}");
    let instance_a = format!("local-connector-a-{test_id}");
    let instance_b = format!("local-connector-b-{test_id}");
    let coordinator_a = ValkeyCoordinator::connect(
        valkey_url.as_str(),
        key_prefix.as_str(),
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect("connect first test coordinator");
    let coordinator_b = ValkeyCoordinator::connect(
        valkey_url.as_str(),
        key_prefix.as_str(),
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect("connect second test coordinator");
    let relay_a = ConnectorRelay::new_distributed(
        None,
        RelayRuntimeLimits::default(),
        instance_a.clone(),
        coordinator_a.clone(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let relay_b = ConnectorRelay::new_distributed(
        None,
        RelayRuntimeLimits::default(),
        instance_b.clone(),
        coordinator_b.clone(),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let listener_a =
        spawn_test_instance_listener(coordinator_a.clone(), instance_a.clone(), relay_a.clone())
            .await;
    let listener_b =
        spawn_test_instance_listener(coordinator_b.clone(), instance_b.clone(), relay_b.clone())
            .await;

    let (device_outbound, mut device_inbound) = mpsc::channel(8);
    relay_b
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-new".to_string(),
            device_outbound,
        )
        .await;
    let stale_presence = DevicePresence {
        instance_id: instance_b.clone(),
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        session_id: "session-old".to_string(),
    };
    let active_presence = DevicePresence {
        session_id: "session-new".to_string(),
        ..stale_presence.clone()
    };
    coordinator_b
        .register_device_presence(&stale_presence)
        .await
        .expect("register stale presence");
    coordinator_b
        .register_device_presence(&active_presence)
        .await
        .expect("replace active presence");
    assert!(!coordinator_b
        .unregister_device_presence(&stale_presence)
        .await
        .expect("reject stale presence cleanup"));
    assert_eq!(
        coordinator_a
            .device_presence("device-1")
            .await
            .expect("load active presence"),
        Some(active_presence.clone())
    );

    let dispatch = {
        let relay = relay_a.clone();
        tokio::spawn(async move {
            relay
                .dispatch(relay_request("request-dispatch"), Duration::from_secs(2))
                .await
        })
    };
    let outbound = device_inbound
        .recv()
        .await
        .expect("receive cross-instance dispatch");
    let outbound: RelayRequest =
        serde_json::from_str(&outbound).expect("parse cross-instance dispatch");
    assert_eq!(outbound.request_id, "request-dispatch");
    assert!(
        relay_b
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-dispatch","status":200,"body":{"adapter_session_id":"adapter-cross-instance"}}"#,
            )
            .await
            .expect("route cross-instance response")
    );
    let response = dispatch
        .await
        .expect("join cross-instance dispatch")
        .expect("complete cross-instance dispatch");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["adapter_session_id"].as_str(),
        Some("adapter-cross-instance")
    );

    let send = {
        let relay = relay_a.clone();
        tokio::spawn(async move { relay.send(relay_request("request-send")).await })
    };
    let outbound = device_inbound
        .recv()
        .await
        .expect("receive cross-instance one-way request");
    let outbound: RelayRequest =
        serde_json::from_str(&outbound).expect("parse cross-instance one-way request");
    assert_eq!(outbound.request_id, "request-send");
    send.await
        .expect("join cross-instance one-way request")
        .expect("receive owner-instance delivery acknowledgement");

    let timed_out_dispatch = {
        let relay = relay_a.clone();
        tokio::spawn(async move {
            relay
                .dispatch(relay_request("request-timeout"), Duration::from_millis(100))
                .await
        })
    };
    let outbound = device_inbound
        .recv()
        .await
        .expect("receive request that will time out");
    let outbound: RelayRequest =
        serde_json::from_str(&outbound).expect("parse request that will time out");
    assert_eq!(outbound.request_id, "request-timeout");
    assert!(matches!(
        timed_out_dispatch
            .await
            .expect("join timed-out dispatch")
            .expect_err("missing client response must time out"),
        RelayError::Timeout
    ));
    assert_eq!(relay_a.stats().await.pending_relay_requests, 0);
    assert!(coordinator_a
        .relay_correlation("request-timeout")
        .await
        .expect("load timed-out correlation")
        .is_none());

    let terminal_subscription = relay_a
        .subscribe_terminal_session_for("terminal-1", "owner-1", "device-1")
        .await
        .expect("subscribe remote terminal session");
    let terminal_subscription_id = terminal_subscription.id;
    let mut terminal_events = terminal_subscription.events;
    assert!(
        relay_b
            .handle_inbound_text(
                r#"{"type":"terminal_output","terminal_session_id":"terminal-1","data":"cross-instance-output"}"#,
            )
            .await
            .expect("publish cross-instance terminal event")
    );
    let terminal_event = tokio::time::timeout(Duration::from_secs(2), terminal_events.recv())
        .await
        .expect("wait for cross-instance terminal event")
        .expect("receive cross-instance terminal event");
    assert_eq!(terminal_event.message_type, "terminal_output");
    assert_eq!(
        terminal_event.body["data"].as_str(),
        Some("cross-instance-output")
    );
    relay_a
        .drop_terminal_subscription("terminal-1", terminal_subscription_id.as_str())
        .await
        .expect("drop remote terminal subscription");

    let missing_subscriber_error = coordinator_a
        .publish_instance_message(
            format!("missing-{test_id}").as_str(),
            &InterInstanceRelayMessage::TerminalEvent {
                event: TerminalRelayEvent {
                    message_type: "terminal_output".to_string(),
                    terminal_session_id: "terminal-missing".to_string(),
                    body: serde_json::json!({"data":"ignored"}),
                },
            },
        )
        .await
        .expect_err("missing target instance must fail");
    assert!(missing_subscriber_error.contains("no active control subscriber"));

    listener_b.abort();
    let _ = listener_b.await;
    let failed_instance_error = relay_a
        .dispatch(
            relay_request("request-failed-instance"),
            Duration::from_secs(1),
        )
        .await
        .expect_err("failed owner instance must reject the relay request");
    assert!(matches!(
        failed_instance_error,
        RelayError::Coordination(ref error)
            if error.contains("no active control subscriber")
    ));
    assert_eq!(relay_a.stats().await.pending_relay_requests, 0);
    assert!(coordinator_a
        .relay_correlation("request-failed-instance")
        .await
        .expect("load failed-instance correlation")
        .is_none());

    assert!(coordinator_b
        .unregister_device_presence(&active_presence)
        .await
        .expect("remove active presence"));
    let expiring_coordinator = ValkeyCoordinator::connect(
        valkey_url.as_str(),
        key_prefix.as_str(),
        Duration::from_secs(1),
        Duration::from_secs(5),
    )
    .await
    .expect("connect expiring presence coordinator");
    let expiring_presence = DevicePresence {
        instance_id: instance_b,
        owner_user_id: "owner-1".to_string(),
        device_id: "device-expiring".to_string(),
        session_id: "session-expiring".to_string(),
    };
    expiring_coordinator
        .register_device_presence(&expiring_presence)
        .await
        .expect("register expiring presence");
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert!(coordinator_a
        .device_presence("device-expiring")
        .await
        .expect("load expired presence")
        .is_none());
    listener_a.abort();
    let _ = listener_a.await;
}

#[tokio::test]
async fn rejects_pending_requests_over_per_device_limit() {
    let relay = ConnectorRelay::new(
        None,
        RelayRuntimeLimits {
            max_pending_requests_per_device: 1,
            ..RelayRuntimeLimits::default()
        },
    );
    let (outbound, mut inbound) = mpsc::channel(8);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;

    let first_dispatch = {
        let relay = relay.clone();
        tokio::spawn(async move {
            relay
                .dispatch(relay_request("request-1"), Duration::from_secs(1))
                .await
        })
    };
    let outbound = inbound.recv().await.expect("first outbound request");
    assert!(outbound.contains("request-1"));

    let second_error = relay
        .dispatch(relay_request("request-2"), Duration::from_millis(250))
        .await
        .expect_err("second request should be rejected");
    assert!(matches!(
        second_error,
        RelayError::TooManyPendingRequests { limit: 1, .. }
    ));

    assert!(
        relay
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-1","status":200,"body":{"adapter_session_id":"adapter-1"}}"#,
            )
            .await
            .expect("complete first request")
    );
    first_dispatch
        .await
        .expect("first dispatch task")
        .expect("first dispatch response");
}

#[tokio::test]
async fn oversized_terminal_event_is_rewritten_to_terminal_error() {
    let relay = ConnectorRelay::new(
        None,
        RelayRuntimeLimits {
            terminal_max_event_bytes: 256,
            ..RelayRuntimeLimits::default()
        },
    );
    let subscription = relay
        .subscribe_terminal_session("terminal-1")
        .await
        .expect("subscribe terminal session");
    let subscription_id = subscription.id;
    let mut receiver = subscription.events;
    assert!(relay
        .handle_inbound_text(
            serde_json::json!({
                "type": "terminal_output",
                "terminal_session_id": "terminal-1",
                "data": "x".repeat(2048),
            })
            .to_string()
            .as_str(),
        )
        .await
        .expect("publish oversized terminal event"));
    let event = receiver.recv().await.expect("terminal event");
    assert_eq!(event.message_type, "terminal_error");
    assert_eq!(
        event.body["original_message_type"].as_str(),
        Some("terminal_output")
    );
    relay
        .drop_terminal_subscription("terminal-1", subscription_id.as_str())
        .await
        .expect("drop terminal subscription");
    let stats = relay.stats().await;
    assert_eq!(stats.terminal_sessions, 0);
    assert_eq!(stats.terminal_ws_subscribers, 0);
}

#[tokio::test]
async fn terminal_subscription_limits_bound_in_memory_state() {
    let relay = ConnectorRelay::new(
        None,
        RelayRuntimeLimits {
            terminal_max_active_sessions: 1,
            terminal_max_subscribers_per_session: 1,
            ..RelayRuntimeLimits::default()
        },
    );
    let first = relay
        .subscribe_terminal_session("terminal-1")
        .await
        .expect("first terminal subscription");

    let subscriber_error = match relay.subscribe_terminal_session("terminal-1").await {
        Ok(_) => panic!("second subscriber must hit the per-session limit"),
        Err(error) => error,
    };
    assert!(subscriber_error.contains("subscriber capacity is exhausted"));
    let session_error = match relay.subscribe_terminal_session("terminal-2").await {
        Ok(_) => panic!("second terminal session must hit the instance limit"),
        Err(error) => error,
    };
    assert!(session_error.contains("session capacity is exhausted"));
    let stats = relay.stats().await;
    assert_eq!(stats.terminal_sessions, 1);
    assert_eq!(stats.terminal_ws_subscribers, 1);

    relay
        .drop_terminal_subscription("terminal-1", first.id.as_str())
        .await
        .expect("drop first terminal subscription");
    relay
        .subscribe_terminal_session("terminal-2")
        .await
        .expect("released capacity must be reusable");
}

#[tokio::test]
async fn terminal_soft_pressure_pauses_only_new_sessions() {
    let relay = ConnectorRelay::new(
        None,
        RelayRuntimeLimits {
            terminal_max_active_sessions: 2,
            terminal_new_session_soft_limit: 1,
            terminal_max_subscribers_per_session: 2,
            ..RelayRuntimeLimits::default()
        },
    );
    let first = relay
        .subscribe_terminal_session("terminal-1")
        .await
        .expect("first terminal session");
    let second_existing = relay
        .subscribe_terminal_session("terminal-1")
        .await
        .expect("existing terminal session remains available under pressure");
    let error = match relay.subscribe_terminal_session("terminal-2").await {
        Ok(_) => panic!("new terminal session must pause at the soft limit"),
        Err(error) => error,
    };
    assert!(error.contains("soft pressure limit"));
    assert!(relay.stats().await.new_terminal_sessions_paused);

    relay
        .drop_terminal_subscription("terminal-1", first.id.as_str())
        .await
        .expect("drop first subscriber");
    relay
        .drop_terminal_subscription("terminal-1", second_existing.id.as_str())
        .await
        .expect("drop second subscriber");
    relay
        .subscribe_terminal_session("terminal-2")
        .await
        .expect("new terminal sessions resume after pressure clears");
}

#[tokio::test]
async fn critical_platform_pressure_pauses_only_new_terminal_sessions() {
    let relay = ConnectorRelay::new(None, RelayRuntimeLimits::default());
    let first = relay
        .subscribe_terminal_session("terminal-1")
        .await
        .expect("first terminal session");

    relay.set_platform_pressure_level(PlatformPressureLevel::Critical);
    let existing = relay
        .subscribe_terminal_session("terminal-1")
        .await
        .expect("existing terminal remains subscribable under critical pressure");
    let error = match relay.subscribe_terminal_session("terminal-2").await {
        Ok(_) => panic!("new terminal session must pause under critical pressure"),
        Err(error) => error,
    };
    assert!(error.contains("platform pressure is critical"));
    assert!(relay.new_terminal_sessions_paused().await);

    relay.set_platform_pressure_level(PlatformPressureLevel::Elevated);
    relay
        .subscribe_terminal_session("terminal-2")
        .await
        .expect("new terminal sessions resume below critical pressure");

    relay
        .drop_terminal_subscription("terminal-1", first.id.as_str())
        .await
        .expect("drop first subscriber");
    relay
        .drop_terminal_subscription("terminal-1", existing.id.as_str())
        .await
        .expect("drop existing subscriber");
}

#[tokio::test]
async fn plugin_response_completes_pending_relay_request() {
    let relay = ConnectorRelay::default();
    let (outbound, mut inbound) = mpsc::channel(1);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;
    let dispatch_relay = relay.clone();
    let dispatch = tokio::spawn(async move {
        dispatch_relay
            .dispatch(relay_request("request-1"), Duration::from_secs(1))
            .await
    });
    let outbound = inbound.recv().await.expect("Plugin relay request");
    assert!(outbound.contains("plugin_prepare_request"));
    assert!(
        relay
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-1","status":200,"body":{"adapter_session_id":"adapter-1"}}"#,
            )
            .await
            .expect("Plugin relay response")
    );

    let response = dispatch.await.expect("dispatch task").expect("response");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["adapter_session_id"].as_str(),
        Some("adapter-1")
    );

    let asset_relay = relay.clone();
    let asset_dispatch = tokio::spawn(async move {
        asset_relay
            .dispatch(
                RelayRequest {
                    message_type: "plugin_ui_asset_request".to_string(),
                    request_id: "request-2".to_string(),
                    owner_user_id: "owner-1".to_string(),
                    device_id: "device-1".to_string(),
                    workspace_id: String::new(),
                    method: "POST".to_string(),
                    path: "/plugins/ui/assets".to_string(),
                    headers: BTreeMap::new(),
                    body: serde_json::json!({"relative_path":"./ui/index.html"}),
                    platform_signature: None,
                    platform_signature_key_id: None,
                    platform_signature_alg: None,
                    platform_timestamp: None,
                    platform_nonce: None,
                },
                Duration::from_secs(1),
            )
            .await
    });
    let outbound = inbound.recv().await.expect("Plugin UI asset relay request");
    assert!(outbound.contains("plugin_ui_asset_request"));
    assert!(
        relay
            .handle_inbound_text(
                r#"{"type":"plugin_ui_asset_response","request_id":"request-2","status":200,"body":{"kind":"entrypoint","body_base64":"PGh0bWw+"}}"#,
            )
            .await
            .expect("Plugin UI asset relay response")
    );
    let response = asset_dispatch
        .await
        .expect("asset dispatch task")
        .expect("asset response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body["kind"].as_str(), Some("entrypoint"));

    for (index, action) in ["list", "read", "create", "update"].into_iter().enumerate() {
        let request_id = format!("artifact-request-{index}");
        let request_type = format!("plugin_artifact_{action}_request");
        let response_type = format!("plugin_artifact_{action}_response");
        let artifact_relay = relay.clone();
        let dispatch_request_id = request_id.clone();
        let dispatch_request_type = request_type.clone();
        let dispatch = tokio::spawn(async move {
            artifact_relay
                .dispatch(
                    RelayRequest {
                        message_type: dispatch_request_type,
                        request_id: dispatch_request_id,
                        owner_user_id: "owner-1".to_string(),
                        device_id: "device-1".to_string(),
                        workspace_id: "workspace-1".to_string(),
                        method: "POST".to_string(),
                        path: format!("/plugins/artifacts/{action}"),
                        headers: BTreeMap::new(),
                        body: serde_json::json!({"access":{"run_id":"run-1"}}),
                        platform_signature: None,
                        platform_signature_key_id: None,
                        platform_signature_alg: None,
                        platform_timestamp: None,
                        platform_nonce: None,
                    },
                    Duration::from_secs(1),
                )
                .await
        });
        let outbound = inbound.recv().await.expect("Plugin Artifact relay request");
        assert!(outbound.contains(request_type.as_str()));
        assert!(relay
            .handle_inbound_text(
                serde_json::json!({
                    "type": response_type,
                    "request_id": request_id,
                    "status": 200,
                    "body": {"action": action},
                })
                .to_string()
                .as_str(),
            )
            .await
            .expect("Plugin Artifact relay response"));
        let response = dispatch
            .await
            .expect("Artifact dispatch task")
            .expect("Artifact response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body["action"].as_str(), Some(action));
    }
}

#[tokio::test]
async fn relay_response_requires_the_original_connector_session() {
    let relay = ConnectorRelay::default();
    let (outbound, mut inbound) = mpsc::channel(1);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;
    let dispatch = {
        let relay = relay.clone();
        tokio::spawn(async move {
            relay
                .dispatch(
                    relay_request("request-source-bound"),
                    Duration::from_secs(1),
                )
                .await
        })
    };
    inbound.recv().await.expect("relay request");
    let response = r#"{"type":"plugin_prepare_response","request_id":"request-source-bound","status":200,"body":{"adapter_session_id":"adapter-1"}}"#;
    let wrong_source = RelaySessionIdentity {
        owner_user_id: "owner-2".to_string(),
        device_id: "device-2".to_string(),
        session_id: "session-2".to_string(),
    };
    let error = relay
        .handle_inbound_text_from(wrong_source, response)
        .await
        .expect_err("another connector session must not complete the request");
    assert!(error.contains("source does not match"));
    assert_eq!(relay.stats().await.pending_relay_requests, 1);

    assert!(relay
        .handle_inbound_text_from(
            RelaySessionIdentity {
                owner_user_id: "owner-1".to_string(),
                device_id: "device-1".to_string(),
                session_id: "session-1".to_string(),
            },
            response,
        )
        .await
        .expect("original connector response"));
    dispatch
        .await
        .expect("dispatch task")
        .expect("bound response");
}

#[tokio::test]
async fn terminal_event_requires_the_subscribed_connector_session() {
    let relay = ConnectorRelay::default();
    let (outbound, _inbound) = mpsc::channel(1);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;
    let subscription = relay
        .subscribe_terminal_session_for("terminal-source-bound", "owner-1", "device-1")
        .await
        .expect("terminal subscription");
    let event = r#"{"type":"terminal_output","terminal_session_id":"terminal-source-bound","data":"hello"}"#;
    let error = relay
        .handle_inbound_text_from(
            RelaySessionIdentity {
                owner_user_id: "owner-2".to_string(),
                device_id: "device-2".to_string(),
                session_id: "session-2".to_string(),
            },
            event,
        )
        .await
        .expect_err("another connector session must not publish terminal output");
    assert!(error.contains("source does not match"));

    assert!(relay
        .handle_inbound_text_from(
            RelaySessionIdentity {
                owner_user_id: "owner-1".to_string(),
                device_id: "device-1".to_string(),
                session_id: "session-1".to_string(),
            },
            event,
        )
        .await
        .expect("original connector terminal event"));
    relay
        .drop_terminal_subscription("terminal-source-bound", subscription.id.as_str())
        .await
        .expect("drop terminal subscription");
}

#[tokio::test]
async fn workspace_directory_responses_complete_pending_relay_requests() {
    let relay = ConnectorRelay::default();
    let (outbound, mut inbound) = mpsc::channel(2);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;

    for action in ["list", "create"] {
        let request_id = format!("workspace-directory-{action}");
        let request_type = format!("workspace_directory_{action}_request");
        let response_type = format!("workspace_directory_{action}_response");
        let dispatch_relay = relay.clone();
        let dispatch_request_id = request_id.clone();
        let dispatch_request_type = request_type.clone();
        let dispatch = tokio::spawn(async move {
            let mut request = relay_request(dispatch_request_id.as_str());
            request.message_type = dispatch_request_type;
            request.method = if action == "list" { "GET" } else { "POST" }.to_string();
            request.path = "/api/local/runtime/workspaces/workspace-1/directories".to_string();
            request.body = serde_json::json!({"path":"apps"});
            dispatch_relay
                .dispatch(request, Duration::from_secs(1))
                .await
        });

        let outbound = inbound.recv().await.expect("workspace directory request");
        assert!(outbound.contains(request_type.as_str()));
        assert!(relay
            .handle_inbound_text(
                serde_json::json!({
                    "type": response_type,
                    "request_id": request_id,
                    "status": 200,
                    "body": {"path":"apps","entries":[]},
                })
                .to_string()
                .as_str(),
            )
            .await
            .expect("workspace directory response"));

        let response = dispatch
            .await
            .expect("workspace directory dispatch task")
            .expect("workspace directory relay response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body["path"].as_str(), Some("apps"));
    }
}

#[tokio::test]
async fn workspace_filesystem_responses_complete_pending_relay_requests() {
    let relay = ConnectorRelay::default();
    let (outbound, mut inbound) = mpsc::channel(16);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;

    for operation in [
        "list",
        "read",
        "search_entries",
        "search_content",
        "create_directory",
        "create_file",
        "write_file",
        "move",
        "delete",
    ] {
        let request_id = format!("workspace-filesystem-{operation}");
        let dispatch_relay = relay.clone();
        let dispatch_request_id = request_id.clone();
        let dispatch = tokio::spawn(async move {
            let mut request = relay_request(dispatch_request_id.as_str());
            request.message_type = "workspace_filesystem_request".to_string();
            request.method = "POST".to_string();
            request.path = "/api/local/runtime/workspaces/workspace-1/filesystem".to_string();
            request.body = serde_json::json!({"operation": operation});
            dispatch_relay
                .dispatch(request, Duration::from_secs(1))
                .await
        });

        let outbound = inbound.recv().await.expect("workspace filesystem request");
        assert!(outbound.contains("workspace_filesystem_request"));
        assert!(outbound.contains(request_id.as_str()));
        assert!(relay
            .handle_inbound_text(
                serde_json::json!({
                    "type": "workspace_filesystem_response",
                    "request_id": request_id,
                    "status": 200,
                    "body": {"operation": operation},
                })
                .to_string()
                .as_str(),
            )
            .await
            .expect("workspace filesystem response"));

        let response = dispatch
            .await
            .expect("workspace filesystem dispatch task")
            .expect("workspace filesystem relay response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body["operation"].as_str(), Some(operation));
    }
}

#[tokio::test]
async fn unknown_relay_response_types_are_rejected_instead_of_acknowledged() {
    let relay = ConnectorRelay::default();
    let error = relay
        .handle_inbound_text(
            r#"{"type":"workspace_future_response","request_id":"request-1","status":200}"#,
        )
        .await
        .expect_err("unknown response must be rejected");
    assert!(error.contains("workspace_future_response"));

    assert!(!relay
        .handle_inbound_text(r#"{"type":"device_status","connected":true}"#)
        .await
        .expect("ordinary control message"));
}

#[tokio::test]
async fn runtime_limits_are_applied_after_hot_reload() {
    let relay = ConnectorRelay::default();
    let (outbound, mut inbound) = mpsc::channel(8);
    relay
        .register_session(
            "device-1".to_string(),
            "owner-1".to_string(),
            "session-1".to_string(),
            outbound,
        )
        .await;

    relay.update_runtime_config(
        None,
        RelayRuntimeLimits {
            max_pending_requests_per_device: 1,
            ..RelayRuntimeLimits::default()
        },
    );

    let first_dispatch = {
        let relay = relay.clone();
        tokio::spawn(async move {
            relay
                .dispatch(relay_request("request-1"), Duration::from_secs(1))
                .await
        })
    };
    let outbound = inbound.recv().await.expect("first outbound request");
    assert!(outbound.contains("request-1"));

    let second_error = relay
        .dispatch(relay_request("request-2"), Duration::from_millis(250))
        .await
        .expect_err("second request should be rejected");
    assert!(matches!(
        second_error,
        RelayError::TooManyPendingRequests { limit: 1, .. }
    ));

    assert!(
        relay
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-1","status":200,"body":{"adapter_session_id":"adapter-1"}}"#,
            )
            .await
            .expect("complete first request")
    );
    first_dispatch
        .await
        .expect("first dispatch task")
        .expect("first dispatch response");
}
