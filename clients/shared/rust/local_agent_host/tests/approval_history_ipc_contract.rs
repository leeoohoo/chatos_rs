// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalApprovalHistoryIpcExecutor,
};
use chatos_local_agent_protocol::{
    AppendApprovalHistoryCommand, ListApprovalHistoryCommand, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse, LocalApprovalHistoryDraft,
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
                encryption_secret: SecretReference::new("test:approval-history-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([113; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> =
        Arc::new(LocalApprovalHistoryIpcExecutor::new(
            storage.clone(),
            scope(),
            "device-1",
            Arc::new(RejectTail),
        ));
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

fn append_command(record_id: &str, decision: &str) -> LocalAgentCommand {
    LocalAgentCommand::AppendApprovalHistory(AppendApprovalHistoryCommand {
        record_id: record_id.to_string(),
        draft: LocalApprovalHistoryDraft {
            command: "git push origin main".to_string(),
            cwd: "/workspace/project".to_string(),
            source: "native-terminal".to_string(),
            mode: "request_approval".to_string(),
            decision: decision.to_string(),
            risk: "high".to_string(),
            reason: Some("confirmed".to_string()),
        },
    })
}

#[tokio::test]
async fn approval_history_is_owner_scoped_and_lists_terminal_decisions() {
    let server = server().await;
    for (id, decision) in [
        ("approval-history-1", "approved"),
        ("approval-history-2", "denied"),
    ] {
        let response = server
            .handle_request(request("alice", id, append_command(id, decision)))
            .await
            .response;
        assert!(matches!(
            response,
            LocalAgentIpcResponse::ApprovalHistory(ref record)
                if record.owner_user_id == "alice" && record.revision == 1
        ));
    }

    let listed = server
        .handle_request(request(
            "alice",
            "list",
            LocalAgentCommand::ListApprovalHistory(ListApprovalHistoryCommand {
                cursor: None,
                limit: 100,
            }),
        ))
        .await
        .response;
    let LocalAgentIpcResponse::ApprovalHistoryRecords {
        records,
        next_cursor,
    } = listed
    else {
        panic!("approval history list did not return records");
    };
    assert_eq!(records.len(), 2);
    assert!(next_cursor.is_none());

    let foreign = server
        .handle_request(request(
            "bob",
            "foreign",
            LocalAgentCommand::ListApprovalHistory(ListApprovalHistoryCommand {
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
