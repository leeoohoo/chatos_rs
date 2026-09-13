// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalTerminalHistoryIpcExecutor,
};
use chatos_local_agent_protocol::{
    AppendTerminalHistoryCommand, DeleteTerminalHistoryCommand, ListTerminalHistoryCommand,
    LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse,
    LocalTerminalHistoryDraft, LOCAL_AGENT_PROTOCOL_VERSION,
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
                encryption_secret: SecretReference::new("test:terminal-history-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([109; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> =
        Arc::new(LocalTerminalHistoryIpcExecutor::new(
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

fn append_command(record_id: &str, command: &str) -> LocalAgentCommand {
    LocalAgentCommand::AppendTerminalHistory(AppendTerminalHistoryCommand {
        record_id: record_id.to_string(),
        draft: LocalTerminalHistoryDraft {
            project_id: Some("project-1".to_string()),
            terminal_session_id: "native-terminal".to_string(),
            command: command.to_string(),
            exit_code: Some(0),
            state: serde_json::json!({"status": "completed"}),
        },
    })
}

#[tokio::test]
async fn terminal_history_is_owner_scoped_listed_deleted_and_cleared() {
    let server = server().await;
    for (id, command) in [("terminal:1", "cargo check"), ("terminal:2", "cargo test")] {
        let response = execute(&server, id, append_command(id, command)).await;
        assert!(matches!(
            response,
            LocalAgentIpcResponse::TerminalHistory(ref record)
                if record.owner_user_id == "alice" && record.revision == 1
        ));
    }

    let listed = execute(
        &server,
        "list",
        LocalAgentCommand::ListTerminalHistory(ListTerminalHistoryCommand {
            cursor: None,
            limit: 100,
        }),
    )
    .await;
    let LocalAgentIpcResponse::TerminalHistoryRecords {
        records,
        next_cursor,
    } = listed
    else {
        panic!("terminal history list did not return records");
    };
    assert_eq!(records.len(), 2);
    assert!(next_cursor.is_none());

    let stale = execute(
        &server,
        "stale",
        LocalAgentCommand::DeleteTerminalHistory(DeleteTerminalHistoryCommand {
            record_id: "terminal:1".to_string(),
            expected_revision: 2,
        }),
    )
    .await;
    assert!(matches!(
        stale,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "terminal_history_conflict"
    ));

    assert!(matches!(
        execute(
            &server,
            "delete",
            LocalAgentCommand::DeleteTerminalHistory(DeleteTerminalHistoryCommand {
                record_id: "terminal:1".to_string(),
                expected_revision: 1,
            }),
        )
        .await,
        LocalAgentIpcResponse::Success
    ));
    assert!(matches!(
        execute(&server, "clear", LocalAgentCommand::ClearTerminalHistory).await,
        LocalAgentIpcResponse::Success
    ));

    let foreign = server
        .handle_request(request(
            "bob",
            "foreign",
            LocalAgentCommand::ListTerminalHistory(ListTerminalHistoryCommand {
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
}
