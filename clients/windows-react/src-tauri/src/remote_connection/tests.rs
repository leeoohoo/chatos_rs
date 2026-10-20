use super::{
    credentials::{tests_support::memory_vault, RemoteCredentialVault},
    models::{RemoteCredentialBundle, RemoteCredentialMutation, RemoteSecretMutation},
    *,
};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalRemoteAuthenticationType, LocalRemoteConnection,
    LocalRemoteHostKeyPolicy,
};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, Mutex as StandardMutex},
};

struct FixedRouteProvider;

impl RemoteConnectorRouteProvider for FixedRouteProvider {
    fn route(&self, owner_user_id: &str) -> Result<RemoteConnectorRoute, String> {
        if owner_user_id != "owner-1" {
            return Err("wrong owner".into());
        }
        Ok(RemoteConnectorRoute {
            device_id: "windows-device-1".into(),
            workspace_id: "windows-local-machine".into(),
        })
    }
}

struct NeverTester;

impl RemoteConnectionTester for NeverTester {
    fn test(
        &self,
        _connection: SshConnectionMaterial,
        _verification_code: Option<String>,
    ) -> ssh::TestFuture<'_> {
        Box::pin(async { panic!("unexpected SSH test") })
    }

    fn reset(&self) -> ssh::ResetFuture<'_> {
        Box::pin(async {})
    }
}

struct CapturingTester {
    calls: StandardMutex<Vec<(SshConnectionMaterial, Option<String>)>>,
    result: PresentedRemoteConnectionTestResult,
}

impl RemoteConnectionTester for CapturingTester {
    fn test(
        &self,
        connection: SshConnectionMaterial,
        verification_code: Option<String>,
    ) -> ssh::TestFuture<'_> {
        self.calls
            .lock()
            .unwrap()
            .push((connection, verification_code));
        let result = self.result.clone();
        Box::pin(async move { Ok(result) })
    }

    fn reset(&self) -> ssh::ResetFuture<'_> {
        Box::pin(async {})
    }
}

struct ScriptedMetadata {
    results: StandardMutex<VecDeque<Result<HostResult, String>>>,
    commands: StandardMutex<Vec<HostCommand>>,
}

impl ScriptedMetadata {
    fn new(results: Vec<Result<HostResult, String>>) -> Self {
        Self {
            results: StandardMutex::new(results.into()),
            commands: StandardMutex::new(Vec::new()),
        }
    }

    fn commands(&self) -> Vec<HostCommand> {
        self.commands.lock().unwrap().clone()
    }
}

impl RemoteConnectionMetadataClient for ScriptedMetadata {
    fn execute(&self, command: HostCommand) -> MetadataFuture<'_> {
        Box::pin(async move {
            self.commands.lock().unwrap().push(command);
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted metadata response")
        })
    }
}

fn record(connection_id: &str, version: u64) -> LocalRemoteConnection {
    LocalRemoteConnection {
        connection_id: connection_id.into(),
        owner_user_id: "owner-1".into(),
        name: "Production".into(),
        host: "server.example.com".into(),
        port: 22,
        username: "deploy".into(),
        authentication_type: LocalRemoteAuthenticationType::Password,
        has_password: true,
        has_private_key_path: true,
        has_certificate_path: true,
        default_remote_path: Some("/srv/app".into()),
        host_key_policy: LocalRemoteHostKeyPolicy::Strict,
        local_connector_device_id: "windows-device-1".into(),
        local_connector_workspace_id: "windows-local-machine".into(),
        jump_enabled: false,
        jump_connection_id: None,
        jump_host: None,
        jump_port: None,
        jump_username: None,
        has_jump_private_key_path: true,
        has_jump_certificate_path: true,
        has_jump_password: true,
        last_active_at_unix_ms: None,
        version,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
    }
}

fn password_input(password: RemoteSecretMutation) -> RemoteConnectionInput {
    RemoteConnectionInput {
        name: Some(" Production ".into()),
        host: " server.example.com ".into(),
        port: 22,
        username: " deploy ".into(),
        authentication_type: LocalRemoteAuthenticationType::Password,
        default_remote_path: Some(" /srv/app ".into()),
        host_key_policy: LocalRemoteHostKeyPolicy::Strict,
        jump_enabled: false,
        jump_connection_id: None,
        jump_host: None,
        jump_port: None,
        jump_username: None,
        credentials: RemoteCredentialMutation {
            password,
            ..RemoteCredentialMutation::default()
        },
    }
}

fn manager_with_vault(vault: Arc<dyn RemoteCredentialVault>) -> RemoteConnectionManager {
    RemoteConnectionManager::with_dependencies(
        vault,
        Arc::new(FixedRouteProvider),
        Arc::new(NeverTester),
    )
}

fn manager_with_tester(
    vault: Arc<dyn RemoteCredentialVault>,
    tester: Arc<dyn RemoteConnectionTester>,
) -> RemoteConnectionManager {
    RemoteConnectionManager::with_dependencies(vault, Arc::new(FixedRouteProvider), tester)
}

#[tokio::test]
async fn create_keeps_secrets_out_of_host_metadata_and_presentation() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![Ok(HostResult::RemoteConnection {
        connection: Some(record("connection-1", 1)),
    })]);

    let presented = manager
        .create(
            &client,
            "owner-1",
            password_input(RemoteSecretMutation::Replace(" top-secret ".into())),
        )
        .await
        .unwrap();

    assert!(presented.has_password);
    assert!(!presented.has_private_key_path);
    assert!(!presented.has_certificate_path);
    let serialized = serde_json::to_string(&presented).unwrap();
    assert!(!serialized.contains("top-secret"));
    assert!(!serialized.contains("localConnectorDevice"));
    assert!(!serialized.contains("localConnectorWorkspace"));
    let credentials = vault.load("owner-1", "connection-1").unwrap().unwrap();
    assert_eq!(credentials.password.as_deref(), Some("top-secret"));

    let commands = client.commands();
    let HostCommand::CreateRemoteConnection(command) = &commands[0] else {
        panic!("expected create command");
    };
    assert_eq!(command.spec.host, "server.example.com");
    assert_eq!(command.spec.username, "deploy");
    assert_eq!(command.spec.local_connector_device_id, "windows-device-1");
    assert_eq!(
        command.spec.local_connector_workspace_id,
        "windows-local-machine"
    );
}

#[tokio::test]
async fn update_restores_previous_credentials_when_host_rejects_version() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &RemoteCredentialBundle {
                password: Some("old-secret".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![
        Ok(HostResult::RemoteConnection {
            connection: Some(record("connection-1", 4)),
        }),
        Err("远端连接已被更新。".into()),
    ]);

    let error = manager
        .update(
            &client,
            "owner-1",
            "connection-1",
            4,
            password_input(RemoteSecretMutation::Replace("new-secret".into())),
        )
        .await
        .unwrap_err();

    assert_eq!(error, "远端连接已被更新。");
    let credentials = vault.load("owner-1", "connection-1").unwrap().unwrap();
    assert_eq!(credentials.password.as_deref(), Some("old-secret"));
    assert!(matches!(
        client.commands().as_slice(),
        [
            HostCommand::GetRemoteConnection(_),
            HostCommand::UpdateRemoteConnection(_)
        ]
    ));
}

#[tokio::test]
async fn update_commits_metadata_version_and_replaces_incompatible_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &RemoteCredentialBundle {
                password: Some("old-secret".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    let manager = manager_with_vault(vault.clone());
    let mut updated = record("connection-1", 5);
    updated.authentication_type = LocalRemoteAuthenticationType::PrivateKey;
    let client = ScriptedMetadata::new(vec![
        Ok(HostResult::RemoteConnection {
            connection: Some(record("connection-1", 4)),
        }),
        Ok(HostResult::RemoteConnection {
            connection: Some(updated),
        }),
    ]);
    let mut input = password_input(RemoteSecretMutation::Keep);
    input.authentication_type = LocalRemoteAuthenticationType::PrivateKey;
    input.credentials.private_key_path =
        RemoteSecretMutation::Replace("C:\\Users\\alice\\.ssh\\id_ed25519".into());

    let presented = manager
        .update(&client, "owner-1", "connection-1", 4, input)
        .await
        .unwrap();

    assert_eq!(presented.version, 5);
    assert!(!presented.has_password);
    assert!(presented.has_private_key_path);
    let stored = vault.load("owner-1", "connection-1").unwrap().unwrap();
    assert!(stored.password.is_none());
    assert_eq!(
        stored.private_key_path.as_deref(),
        Some("C:\\Users\\alice\\.ssh\\id_ed25519")
    );
    let commands = client.commands();
    let HostCommand::UpdateRemoteConnection(command) = &commands[1] else {
        panic!("expected update command");
    };
    assert_eq!(command.expected_version, 4);
    assert_eq!(
        command.spec.authentication_type,
        LocalRemoteAuthenticationType::PrivateKey
    );
}

#[tokio::test]
async fn delete_restores_credentials_when_host_delete_fails() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &RemoteCredentialBundle {
                password: Some("secret".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![
        Ok(HostResult::RemoteConnection {
            connection: Some(record("connection-1", 3)),
        }),
        Err("删除冲突。".into()),
    ]);

    let error = manager
        .delete(&client, "owner-1", "connection-1", 3)
        .await
        .unwrap_err();

    assert_eq!(error, "删除冲突。");
    assert_eq!(
        vault
            .load("owner-1", "connection-1")
            .unwrap()
            .unwrap()
            .password
            .as_deref(),
        Some("secret")
    );
}

#[tokio::test]
async fn delete_commits_host_version_and_removes_local_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &RemoteCredentialBundle {
                password: Some("secret".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![
        Ok(HostResult::RemoteConnection {
            connection: Some(record("connection-1", 3)),
        }),
        Ok(HostResult::RemoteConnectionDeleted {
            connection_id: "connection-1".into(),
        }),
    ]);

    let deleted = manager
        .delete(&client, "owner-1", "connection-1", 3)
        .await
        .unwrap();

    assert_eq!(deleted, "connection-1");
    assert!(vault.load("owner-1", "connection-1").unwrap().is_none());
    let commands = client.commands();
    let HostCommand::DeleteRemoteConnection(command) = &commands[1] else {
        panic!("expected delete command");
    };
    assert_eq!(command.expected_version, 3);
}

#[tokio::test]
async fn get_removes_a_local_credential_when_host_no_longer_has_the_connection() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &RemoteCredentialBundle {
                password: Some("orphaned".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![Ok(HostResult::RemoteConnection { connection: None })]);

    assert!(manager
        .get(&client, "owner-1", "connection-1")
        .await
        .unwrap()
        .is_none());
    assert!(vault.load("owner-1", "connection-1").unwrap().is_none());
}

#[tokio::test]
async fn list_uses_only_local_credential_flags_and_removes_stale_vault_entries() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    for connection_id in ["connection-1", "stale-connection"] {
        vault
            .save(
                "owner-1",
                connection_id,
                &RemoteCredentialBundle {
                    password: Some(format!("secret-{connection_id}")),
                    ..RemoteCredentialBundle::default()
                },
            )
            .unwrap();
    }
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![Ok(HostResult::RemoteConnections {
        connections: vec![record("connection-1", 1)],
    })]);

    let listed = manager.list(&client, "owner-1").await.unwrap();

    assert_eq!(listed.len(), 1);
    assert!(listed[0].has_password);
    assert!(!listed[0].has_private_key_path);
    assert!(!listed[0].has_jump_password);
    assert!(vault.load("owner-1", "stale-connection").unwrap().is_none());
}

#[tokio::test]
async fn update_rejects_stale_expected_version_before_touching_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &RemoteCredentialBundle {
                password: Some("old-secret".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![Ok(HostResult::RemoteConnection {
        connection: Some(record("connection-1", 7)),
    })]);

    let error = manager
        .update(
            &client,
            "owner-1",
            "connection-1",
            6,
            password_input(RemoteSecretMutation::Replace("new-secret".into())),
        )
        .await
        .unwrap_err();

    assert!(error.contains("刷新"));
    assert_eq!(client.commands().len(), 1);
    assert_eq!(
        vault
            .load("owner-1", "connection-1")
            .unwrap()
            .unwrap()
            .password
            .as_deref(),
        Some("old-secret")
    );
}

#[tokio::test]
async fn referenced_jump_connection_is_owner_scoped_and_supplies_route_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    let manager = manager_with_vault(vault.clone());
    let mut jump = record("jump-1", 2);
    jump.host = "jump.example.com".into();
    jump.port = 2202;
    jump.username = "jumper".into();
    let client = ScriptedMetadata::new(vec![
        Ok(HostResult::RemoteConnection {
            connection: Some(jump),
        }),
        Ok(HostResult::RemoteConnection {
            connection: Some(record("connection-1", 1)),
        }),
    ]);
    let mut input = password_input(RemoteSecretMutation::Replace("secret".into()));
    input.jump_enabled = true;
    input.jump_connection_id = Some("jump-1".into());
    input.jump_host = Some("untrusted.example.com".into());
    input.jump_port = Some(1234);
    input.jump_username = Some("untrusted".into());
    input.credentials.jump_password = RemoteSecretMutation::Replace("must-not-store".into());

    manager.create(&client, "owner-1", input).await.unwrap();

    let commands = client.commands();
    let HostCommand::CreateRemoteConnection(command) = &commands[1] else {
        panic!("expected create command");
    };
    assert_eq!(command.spec.jump_host.as_deref(), Some("jump.example.com"));
    assert_eq!(command.spec.jump_port, Some(2202));
    assert_eq!(command.spec.jump_username.as_deref(), Some("jumper"));
    let stored = vault.load("owner-1", "connection-1").unwrap().unwrap();
    assert!(stored.jump_password.is_none());
}

#[test]
fn credential_index_and_accounts_are_owner_scoped() {
    let directory = tempfile::tempdir().unwrap();
    let index_path = directory.path().join("credentials.json");
    let vault = memory_vault(index_path.clone());
    let first = RemoteCredentialBundle {
        password: Some("first".into()),
        ..RemoteCredentialBundle::default()
    };
    let second = RemoteCredentialBundle {
        password: Some("second".into()),
        ..RemoteCredentialBundle::default()
    };
    vault.save("owner-1", "same-id", &first).unwrap();
    vault.save("owner-2", "same-id", &second).unwrap();

    let persisted_index = std::fs::read_to_string(index_path).unwrap();
    assert!(!persisted_index.contains("first"));
    assert!(!persisted_index.contains("second"));

    assert_eq!(
        vault
            .load("owner-1", "same-id")
            .unwrap()
            .unwrap()
            .password
            .as_deref(),
        Some("first")
    );
    assert_eq!(
        vault
            .load("owner-2", "same-id")
            .unwrap()
            .unwrap()
            .password
            .as_deref(),
        Some("second")
    );
    vault.reconcile("owner-1", &BTreeSet::new()).unwrap();
    assert!(vault.load("owner-1", "same-id").unwrap().is_none());
    assert!(vault.load("owner-2", "same-id").unwrap().is_some());
}

struct FailingSaveVault {
    deleted: StandardMutex<BTreeSet<String>>,
}

impl RemoteCredentialVault for FailingSaveVault {
    fn load(
        &self,
        _owner_user_id: &str,
        _connection_id: &str,
    ) -> Result<Option<RemoteCredentialBundle>, String> {
        Ok(None)
    }

    fn save(
        &self,
        _owner_user_id: &str,
        _connection_id: &str,
        _credentials: &RemoteCredentialBundle,
    ) -> Result<(), String> {
        Err("credential failure".into())
    }

    fn delete(&self, owner_user_id: &str, connection_id: &str) -> Result<(), String> {
        self.deleted
            .lock()
            .unwrap()
            .insert(format!("{owner_user_id}:{connection_id}"));
        Ok(())
    }

    fn reconcile(
        &self,
        _owner_user_id: &str,
        _active_connection_ids: &BTreeSet<String>,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::test]
async fn create_rolls_back_host_metadata_when_credential_save_fails() {
    let vault = Arc::new(FailingSaveVault {
        deleted: StandardMutex::new(BTreeSet::new()),
    });
    let manager = manager_with_vault(vault.clone());
    let client = ScriptedMetadata::new(vec![
        Ok(HostResult::RemoteConnection {
            connection: Some(record("connection-1", 1)),
        }),
        Ok(HostResult::RemoteConnectionDeleted {
            connection_id: "connection-1".into(),
        }),
    ]);

    let error = manager
        .create(
            &client,
            "owner-1",
            password_input(RemoteSecretMutation::Replace("secret".into())),
        )
        .await
        .unwrap_err();

    assert_eq!(error, "credential failure");
    let commands = client.commands();
    assert!(matches!(
        commands.as_slice(),
        [
            HostCommand::CreateRemoteConnection(_),
            HostCommand::DeleteRemoteConnection(_)
        ]
    ));
    assert!(vault
        .deleted
        .lock()
        .unwrap()
        .contains("owner-1:connection-1"));
}

#[tokio::test]
async fn create_requires_the_mac_source_of_truth_authentication_material() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    let manager = manager_with_vault(vault);
    let client = ScriptedMetadata::new(Vec::new());

    let error = manager
        .create(
            &client,
            "owner-1",
            password_input(RemoteSecretMutation::Keep),
        )
        .await
        .unwrap_err();

    assert_eq!(error, "请输入登录密码。");
    assert!(client.commands().is_empty());
}

#[tokio::test]
async fn create_rejects_a_secret_larger_than_the_credential_manager_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    let manager = manager_with_vault(vault);
    let client = ScriptedMetadata::new(Vec::new());

    let error = manager
        .create(
            &client,
            "owner-1",
            password_input(RemoteSecretMutation::Replace("x".repeat(1_025))),
        )
        .await
        .unwrap_err();

    assert_eq!(error, "登录密码无效。");
    assert!(client.commands().is_empty());
}

#[test]
fn credentials_never_serialize_into_the_presented_record() {
    let presented = PresentedRemoteConnection::from_record(
        record("connection-1", 1),
        RemoteCredentialFlags {
            has_password: true,
            ..RemoteCredentialFlags::default()
        },
    );
    let value = serde_json::to_value(presented).unwrap();
    let object = value.as_object().unwrap();
    assert_eq!(
        object.get("hasPassword").and_then(|value| value.as_bool()),
        Some(true)
    );
    for forbidden in [
        "password",
        "privateKeyPath",
        "certificatePath",
        "jumpPassword",
        "jumpPrivateKeyPath",
        "jumpCertificatePath",
        "localConnectorDeviceId",
        "localConnectorWorkspaceId",
    ] {
        assert!(
            !object.contains_key(forbidden),
            "unexpected field {forbidden}"
        );
    }
}

#[tokio::test]
async fn saved_connection_test_resolves_current_jump_metadata_and_device_local_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    vault
        .save(
            "owner-1",
            "connection-1",
            &RemoteCredentialBundle {
                password: Some("target-secret".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    vault
        .save(
            "owner-1",
            "jump-1",
            &RemoteCredentialBundle {
                private_key_path: Some("C:\\Keys\\jump".into()),
                ..RemoteCredentialBundle::default()
            },
        )
        .unwrap();
    let tester = Arc::new(CapturingTester {
        calls: StandardMutex::new(Vec::new()),
        result: PresentedRemoteConnectionTestResult::success(),
    });
    let manager = manager_with_tester(vault, tester.clone());
    let mut target = record("connection-1", 3);
    target.jump_enabled = true;
    target.jump_connection_id = Some("jump-1".into());
    target.jump_host = Some("stale.example.com".into());
    target.jump_port = Some(22);
    target.jump_username = Some("stale".into());
    let mut jump = record("jump-1", 2);
    jump.host = "jump.example.com".into();
    jump.port = 2202;
    jump.username = "jumper".into();
    jump.authentication_type = LocalRemoteAuthenticationType::PrivateKey;
    let client = ScriptedMetadata::new(vec![
        Ok(HostResult::RemoteConnection {
            connection: Some(target),
        }),
        Ok(HostResult::RemoteConnection {
            connection: Some(jump),
        }),
    ]);

    let result = manager
        .test_saved(&client, "owner-1", "connection-1", Some(" 123456 ".into()))
        .await
        .unwrap();

    assert!(result.success);
    let calls = tester.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let (material, verification_code) = &calls[0];
    assert_eq!(verification_code.as_deref(), Some(" 123456 "));
    assert_eq!(material.spec.jump_host.as_deref(), Some("jump.example.com"));
    assert_eq!(material.spec.jump_port, Some(2202));
    assert_eq!(material.spec.jump_username.as_deref(), Some("jumper"));
    assert_eq!(
        material.credentials.password.as_deref(),
        Some("target-secret")
    );
    assert_eq!(
        material
            .jump_credentials
            .as_ref()
            .and_then(|credentials| credentials.private_key_path.as_deref()),
        Some("C:\\Keys\\jump")
    );
    assert_eq!(
        material.jump_authentication_type,
        Some(LocalRemoteAuthenticationType::PrivateKey)
    );
}

#[tokio::test]
async fn draft_connection_test_never_writes_host_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Arc::new(memory_vault(directory.path().join("credentials.json")));
    let tester = Arc::new(CapturingTester {
        calls: StandardMutex::new(Vec::new()),
        result: PresentedRemoteConnectionTestResult::verification("OTP code:".into()),
    });
    let manager = manager_with_tester(vault, tester);
    let client = ScriptedMetadata::new(Vec::new());

    let result = manager
        .test_draft(
            &client,
            "owner-1",
            password_input(RemoteSecretMutation::Replace("secret".into())),
            None,
        )
        .await
        .unwrap();

    assert!(result.verification_required);
    assert_eq!(result.verification_prompt.as_deref(), Some("OTP code:"));
    assert!(client.commands().is_empty());
}
