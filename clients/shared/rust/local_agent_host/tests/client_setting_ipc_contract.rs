// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalClientSettingIpcExecutor,
};
use chatos_local_agent_protocol::{
    DeleteClientSettingCommand, GetClientSettingCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcRequest, LocalAgentIpcResponse, PutClientSettingCommand,
    LOCAL_AGENT_PROTOCOL_VERSION,
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
                encryption_secret: SecretReference::new("test:client-setting-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([103; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> =
        Arc::new(LocalClientSettingIpcExecutor::new(
            storage.clone(),
            scope(),
            "device-1",
            Arc::new(RejectTail),
        ));
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

async fn execute(
    server: &LocalAgentIpcServer,
    id: &str,
    command: LocalAgentCommand,
) -> LocalAgentIpcResponse {
    server
        .handle_request(request("alice", id, command))
        .await
        .response
}

#[tokio::test]
async fn client_setting_is_owner_scoped_revision_bound_and_deletable() {
    let server = server().await;
    let value = serde_json::json!({"projects": {"project-1": {"target": "web"}}});

    let created = execute(
        &server,
        "create",
        LocalAgentCommand::PutClientSetting(PutClientSettingCommand {
            key: "project_run.preferences".to_string(),
            expected_revision: None,
            value: value.clone(),
        }),
    )
    .await;
    assert!(matches!(
        created,
        LocalAgentIpcResponse::ClientSetting(ref setting)
            if setting.revision == 1 && setting.value == value
    ));

    let duplicate = execute(
        &server,
        "duplicate",
        LocalAgentCommand::PutClientSetting(PutClientSettingCommand {
            key: "project_run.preferences".to_string(),
            expected_revision: None,
            value: serde_json::json!({"duplicate": true}),
        }),
    )
    .await;
    assert!(matches!(
        duplicate,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "client_setting_revision_conflict"
    ));

    let updated = execute(
        &server,
        "update",
        LocalAgentCommand::PutClientSetting(PutClientSettingCommand {
            key: "project_run.preferences".to_string(),
            expected_revision: Some(1),
            value: serde_json::json!({"projects": {"project-1": {"target": "desktop"}}}),
        }),
    )
    .await;
    assert!(matches!(
        updated,
        LocalAgentIpcResponse::ClientSetting(ref setting)
            if setting.revision == 2 && setting.value["projects"]["project-1"]["target"] == "desktop"
    ));

    let fetched = execute(
        &server,
        "get",
        LocalAgentCommand::GetClientSetting(GetClientSettingCommand {
            key: "project_run.preferences".to_string(),
        }),
    )
    .await;
    assert!(matches!(
        fetched,
        LocalAgentIpcResponse::ClientSetting(ref setting)
            if setting.owner_user_id == "alice" && setting.revision == 2
    ));

    let foreign = server
        .handle_request(request(
            "bob",
            "foreign",
            LocalAgentCommand::GetClientSetting(GetClientSettingCommand {
                key: "project_run.preferences".to_string(),
            }),
        ))
        .await;
    assert!(matches!(
        foreign.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "owner_scope_mismatch"
    ));

    let stale_delete = execute(
        &server,
        "stale-delete",
        LocalAgentCommand::DeleteClientSetting(DeleteClientSettingCommand {
            key: "project_run.preferences".to_string(),
            expected_revision: 1,
        }),
    )
    .await;
    assert!(matches!(
        stale_delete,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "client_setting_revision_conflict"
    ));

    let deleted = execute(
        &server,
        "delete",
        LocalAgentCommand::DeleteClientSetting(DeleteClientSettingCommand {
            key: "project_run.preferences".to_string(),
            expected_revision: 2,
        }),
    )
    .await;
    assert!(matches!(deleted, LocalAgentIpcResponse::Success));
    let missing = execute(
        &server,
        "missing",
        LocalAgentCommand::GetClientSetting(GetClientSettingCommand {
            key: "project_run.preferences".to_string(),
        }),
    )
    .await;
    assert!(matches!(
        missing,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "client_setting_not_found"
    ));
}
