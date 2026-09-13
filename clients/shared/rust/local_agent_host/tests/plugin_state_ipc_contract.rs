// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalInstalledPluginIpcExecutor,
};
use chatos_local_agent_protocol::{
    DeleteInstalledPluginCommand, ListInstalledPluginsCommand, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse, LocalInstalledPluginDraft,
    PutInstalledPluginCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};

struct RejectTail;

#[async_trait]
impl LocalAgentIpcMutationExecutor for RejectTail {
    async fn execute_mutation(
        &self,
        _request_id: &str,
        _command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        Err(LocalAgentIpcError {
            code: "unsupported".to_string(),
            message: "unsupported".to_string(),
            retryable: false,
        })
    }
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "alice".to_string(),
    }
}

fn request(owner: &str, id: &str, command: LocalAgentCommand) -> LocalAgentIpcRequest {
    LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: id.to_string(),
        owner_user_id: owner.to_string(),
        command,
    }
}

async fn server() -> LocalAgentIpcServer {
    let directory = tempfile::tempdir().unwrap().keep();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:installed-plugin-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([127; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> =
        Arc::new(LocalInstalledPluginIpcExecutor::new(
            storage.clone(),
            scope(),
            "device-1",
            Arc::new(RejectTail),
        ));
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

fn draft(enabled: bool) -> LocalInstalledPluginDraft {
    LocalInstalledPluginDraft {
        plugin_id: "plugin-1".to_string(),
        release: "release-1".to_string(),
        enabled,
        installation: serde_json::json!({
            "pluginID": "plugin-1",
            "releaseID": "release-1",
            "version": "1.2.3",
            "artifactSHA256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "installationPath": "/plugins/plugin-1/1.2.3",
            "installedAt": "2026-09-14T02:00:00Z"
        }),
    }
}

#[tokio::test]
async fn installed_plugin_is_owner_scoped_revision_bound_and_deletable() {
    let server = server().await;
    let created = server
        .handle_request(request(
            "alice",
            "create",
            LocalAgentCommand::PutInstalledPlugin(PutInstalledPluginCommand {
                expected_revision: None,
                draft: draft(true),
            }),
        ))
        .await;
    assert!(matches!(
        created.response,
        LocalAgentIpcResponse::InstalledPlugin(ref record)
            if record.owner_user_id == "alice" && record.revision == 1 && record.draft.enabled
    ));

    let updated = server
        .handle_request(request(
            "alice",
            "update",
            LocalAgentCommand::PutInstalledPlugin(PutInstalledPluginCommand {
                expected_revision: Some(1),
                draft: draft(false),
            }),
        ))
        .await;
    assert!(matches!(
        updated.response,
        LocalAgentIpcResponse::InstalledPlugin(ref record)
            if record.revision == 2 && !record.draft.enabled
    ));

    let listed = server
        .handle_request(request(
            "alice",
            "list",
            LocalAgentCommand::ListInstalledPlugins(ListInstalledPluginsCommand {
                cursor: None,
                limit: 100,
            }),
        ))
        .await;
    assert!(matches!(
        listed.response,
        LocalAgentIpcResponse::InstalledPluginRecords { ref records, next_cursor: None }
            if records.len() == 1 && records[0].draft.plugin_id == "plugin-1"
    ));

    let foreign = server
        .handle_request(request(
            "bob",
            "foreign",
            LocalAgentCommand::ListInstalledPlugins(ListInstalledPluginsCommand {
                cursor: None,
                limit: 100,
            }),
        ))
        .await;
    assert!(matches!(
        foreign.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "owner_scope_mismatch"
    ));

    let deleted = server
        .handle_request(request(
            "alice",
            "delete",
            LocalAgentCommand::DeleteInstalledPlugin(DeleteInstalledPluginCommand {
                plugin_id: "plugin-1".to_string(),
                expected_revision: 2,
            }),
        ))
        .await;
    assert_eq!(deleted.response, LocalAgentIpcResponse::Success);
}
