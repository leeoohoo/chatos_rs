use super::process::{
    snapshot_state, trim_events, ManagedRemoteTerminal, RemoteTerminalEvent,
    RemoteTerminalSessionState,
};
use super::*;
use crate::remote_connection::{
    credentials::{tests_support::memory_vault, RemoteCredentialVault},
    ssh::{RemoteConnectionTester, ResetFuture, TestFuture},
    MetadataFuture, RemoteConnectorRoute, RemoteConnectorRouteProvider,
};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalRemoteAuthenticationType, LocalRemoteConnection,
    LocalRemoteHostKeyPolicy,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

fn state() -> RemoteTerminalSessionState {
    RemoteTerminalSessionState {
        running: true,
        exit_code: None,
        events: VecDeque::new(),
        next_cursor: 1,
        retained_output_bytes: 0,
    }
}

fn append(state: &mut RemoteTerminalSessionState, bytes: Vec<u8>) {
    let cursor = state.next_cursor;
    state.next_cursor += 1;
    state.retained_output_bytes += bytes.len();
    state
        .events
        .push_back(RemoteTerminalEvent::Output { cursor, bytes });
    trim_events(state);
}

struct FixedRoute;

impl RemoteConnectorRouteProvider for FixedRoute {
    fn route(&self, _owner_user_id: &str) -> Result<RemoteConnectorRoute, String> {
        Ok(RemoteConnectorRoute {
            device_id: "device-1".into(),
            workspace_id: "workspace-1".into(),
        })
    }
}

struct FixedMetadata;

impl RemoteConnectionMetadataClient for FixedMetadata {
    fn execute(&self, command: HostCommand) -> MetadataFuture<'_> {
        Box::pin(async move {
            assert!(matches!(command, HostCommand::GetRemoteConnection(_)));
            Ok(HostResult::RemoteConnection {
                connection: Some(LocalRemoteConnection {
                    connection_id: "connection-1".into(),
                    owner_user_id: "owner-1".into(),
                    name: "Server".into(),
                    host: "server.example.com".into(),
                    port: 22,
                    username: "deploy".into(),
                    authentication_type: LocalRemoteAuthenticationType::Password,
                    has_password: true,
                    has_private_key_path: false,
                    has_certificate_path: false,
                    default_remote_path: Some("/srv/app".into()),
                    host_key_policy: LocalRemoteHostKeyPolicy::Strict,
                    local_connector_device_id: "device-1".into(),
                    local_connector_workspace_id: "workspace-1".into(),
                    jump_enabled: false,
                    jump_connection_id: None,
                    jump_host: None,
                    jump_port: None,
                    jump_username: None,
                    has_jump_private_key_path: false,
                    has_jump_certificate_path: false,
                    has_jump_password: false,
                    last_active_at_unix_ms: None,
                    version: 1,
                    created_at_unix_ms: 1,
                    updated_at_unix_ms: 1,
                }),
            })
        })
    }
}

struct VerificationTester {
    calls: AtomicUsize,
}

impl RemoteConnectionTester for VerificationTester {
    fn test(
        &self,
        _connection: SshConnectionMaterial,
        _verification_code: Option<String>,
    ) -> TestFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(
                super::super::models::PresentedRemoteConnectionTestResult::verification(
                    "OTP".into(),
                ),
            )
        })
    }

    fn reset(&self) -> ResetFuture<'_> {
        Box::pin(async {})
    }
}

fn manager_with_verification_tester(tester: Arc<VerificationTester>) -> RemoteConnectionManager {
    let directory = tempfile::tempdir().unwrap().keep();
    let vault = Arc::new(memory_vault(directory.join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &super::super::models::RemoteCredentialBundle {
                password: Some("secret".into()),
                ..super::super::models::RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    RemoteConnectionManager::with_dependencies(vault, Arc::new(FixedRoute), tester)
}

#[test]
fn dimensions_match_macos_bounds() {
    assert_eq!(validate_dimensions(1, 1).unwrap(), (1, 1));
    assert_eq!(validate_dimensions(1_000, 1_000).unwrap(), (1_000, 1_000));
    assert!(validate_dimensions(0, 24).is_err());
    assert!(validate_dimensions(80, 1_001).is_err());
}

#[test]
fn presented_output_event_uses_the_react_wire_field_name() {
    let value = serde_json::to_value(PresentedRemoteTerminalEvent::Output {
        cursor: 1,
        data_base64: "YQ==".into(),
    })
    .unwrap();
    assert_eq!(value["dataBase64"], "YQ==");
    assert!(value.get("data_base64").is_none());
}

#[test]
fn terminal_output_ring_is_bounded_and_reports_cursor_truncation() {
    let mut state = state();
    for _ in 0..=MAXIMUM_OUTPUT_EVENTS {
        append(&mut state, vec![b'x'; 8]);
    }
    assert_eq!(state.events.len(), MAXIMUM_OUTPUT_EVENTS);
    assert_eq!(state.events.front().unwrap().cursor(), 2);
    assert_eq!(state.retained_output_bytes, MAXIMUM_OUTPUT_EVENTS * 8);

    let snapshot = snapshot_state(&state, 0);
    assert!(snapshot.truncated);
    assert_eq!(snapshot.events.first().unwrap().cursor(), 2);
    assert_eq!(snapshot.next_cursor, 129);
    assert_eq!(snapshot.events.len(), MAXIMUM_POLL_EVENTS);
}

#[test]
fn output_ring_enforces_byte_budget() {
    let mut state = state();
    for _ in 0..300 {
        append(&mut state, vec![b'x'; OUTPUT_READ_BYTES]);
    }
    assert!(state.retained_output_bytes <= MAXIMUM_RETAINED_OUTPUT_BYTES);
    assert!(state.events.len() <= MAXIMUM_OUTPUT_EVENTS);
}

#[test]
fn input_and_opaque_identity_validation_fail_closed() {
    assert!(validate_presentation_id("terminal-window-1").is_ok());
    assert!(validate_presentation_id("bad\nwindow").is_err());
    assert!(validate_session_id("remote-terminal-1234-abcd").is_ok());
    assert!(validate_session_id("terminal-1234").is_err());
}

#[tokio::test]
async fn newer_connect_generation_rejects_an_older_completion() {
    let registry = RemoteTerminalRegistry::default();
    let first = registry
        .begin_connect("owner-1", "window-1", "connection-1")
        .await
        .unwrap();
    let second = registry
        .begin_connect("owner-1", "window-1", "connection-2")
        .await
        .unwrap();

    registry.finish_connect(&first).await;
    let state = registry.state.lock().await;
    let lifecycle = state.presentations.get(&second.key).unwrap();
    assert_eq!(lifecycle.generation, second.generation);
    assert_eq!(lifecycle.connect_request_id, Some(second.request_id));
}

#[tokio::test]
async fn reset_invalidates_every_in_flight_connect() {
    let registry = RemoteTerminalRegistry::default();
    let token = registry
        .begin_connect("owner-1", "window-1", "connection-1")
        .await
        .unwrap();
    registry.reset().await;
    registry.finish_connect(&token).await;

    let state = registry.state.lock().await;
    assert!(state.presentations.is_empty());
    assert!(state.sessions.is_empty());
    assert_ne!(state.epoch, token.epoch);
}

#[tokio::test]
async fn in_flight_connects_are_included_in_the_native_session_limit() {
    let registry = RemoteTerminalRegistry::default();
    for index in 0..MAXIMUM_ACTIVE_SESSIONS {
        registry
            .begin_connect("owner-1", &format!("window-{index}"), "connection-1")
            .await
            .unwrap();
    }

    let error = registry
        .begin_connect("owner-1", "one-too-many", "connection-1")
        .await
        .unwrap_err();
    assert!(error.contains("过多"));
}

#[tokio::test]
async fn terminal_start_uses_the_saved_connection_probe_for_mfa() {
    let tester = Arc::new(VerificationTester {
        calls: AtomicUsize::new(0),
    });
    let manager = manager_with_verification_tester(tester.clone());

    let response = manager
        .start_terminal(
            &FixedMetadata,
            "owner-1",
            "window-1",
            "connection-1",
            None,
            80,
            24,
        )
        .await
        .unwrap();

    assert_eq!(
        response,
        PresentedRemoteTerminalStart::VerificationRequired {
            prompt: "OTP".into()
        }
    );
    assert_eq!(tester.calls.load(Ordering::SeqCst), 1);
    let state = manager.terminal_sessions.state.lock().await;
    assert!(state.sessions.is_empty());
    assert!(state
        .presentations
        .values()
        .all(|lifecycle| lifecycle.connect_request_id.is_none()));
}

#[cfg(unix)]
#[tokio::test]
async fn native_pty_streams_output_and_termination_as_ordered_events() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = NativeOpenSshTester::new(directory.path().to_path_buf());
    let material = SshConnectionMaterial {
        spec: chatos_local_agent_protocol::LocalRemoteConnectionSpec {
            name: Some("Server".into()),
            host: "server.example.com".into(),
            port: 22,
            username: "deploy".into(),
            authentication_type: LocalRemoteAuthenticationType::Password,
            default_remote_path: Some("/srv/app".into()),
            host_key_policy: LocalRemoteHostKeyPolicy::Strict,
            local_connector_device_id: "device-1".into(),
            local_connector_workspace_id: "workspace-1".into(),
            jump_enabled: false,
            jump_connection_id: None,
            jump_host: None,
            jump_port: None,
            jump_username: None,
        },
        credentials: super::super::models::RemoteCredentialBundle {
            password: Some("secret".into()),
            ..super::super::models::RemoteCredentialBundle::default()
        },
        jump_authentication_type: None,
        jump_credentials: None,
    };
    let mut launch = runtime.prepare_terminal_launch(&material, None).unwrap();
    assert_eq!(
        launch.arguments.first().and_then(|value| value.to_str()),
        Some("-q")
    );
    launch.executable = "/bin/sh".into();
    launch.arguments = vec![
        "-lc".into(),
        "printf '__CHATOS_PTY_TEST__\\n'; exit 7".into(),
    ];
    let session = ManagedRemoteTerminal::spawn(
        RemoteTerminalIdentity {
            session_id: "remote-terminal-test".into(),
            owner_user_id: "owner-1".into(),
            presentation_id: "window-1".into(),
            connection_id: "connection-1".into(),
            epoch: 0,
            generation: 1,
        },
        launch,
        80,
        24,
    )
    .unwrap();

    let mut cursor = 0;
    let mut output = Vec::new();
    let exit_code = loop {
        let (snapshot, _) = session.poll(cursor, 2_000).await.unwrap();
        for event in snapshot.events {
            cursor = event.cursor();
            match event {
                RemoteTerminalEvent::Output { bytes, .. } => output.extend(bytes),
                RemoteTerminalEvent::Terminated { .. } => {}
            }
        }
        if !snapshot.running {
            break snapshot.exit_code;
        }
    };

    assert!(String::from_utf8_lossy(&output).contains("__CHATOS_PTY_TEST__"));
    assert_eq!(exit_code, Some(7));
    session.close();
}
