// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalStoryIpcExecutor,
};
use chatos_local_agent_protocol::{
    DeleteStoryCommand, GetStoryCommand, ListStoriesCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcRequest, LocalAgentIpcResponse, LocalStoryDraft, LocalStoryKind, PutStoryCommand,
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

fn request(
    owner_user_id: &str,
    request_id: &str,
    command: LocalAgentCommand,
) -> LocalAgentIpcRequest {
    LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: request_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        command,
    }
}

fn draft(kind: LocalStoryKind, state_version: u64) -> LocalStoryDraft {
    LocalStoryDraft {
        project_id: "project-1".to_string(),
        kind,
        status: Some("draft".to_string()),
        state: serde_json::json!({"version": state_version}),
    }
}

async fn server() -> LocalAgentIpcServer {
    let directory = tempfile::tempdir().unwrap().keep();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:story-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([89; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> = Arc::new(LocalStoryIpcExecutor::new(
        storage.clone(),
        scope(),
        "device-1",
        Arc::new(RejectTail),
    ));
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

async fn put(
    server: &LocalAgentIpcServer,
    request_id: &str,
    record_id: &str,
    expected_revision: Option<u64>,
    draft: LocalStoryDraft,
) -> LocalAgentIpcResponse {
    server
        .handle_request(request(
            "alice",
            request_id,
            LocalAgentCommand::PutStory(PutStoryCommand {
                record_id: record_id.to_string(),
                expected_revision,
                draft,
            }),
        ))
        .await
        .response
}

#[tokio::test]
async fn story_create_and_update_require_revision_cas() {
    let server = server().await;
    let created = put(
        &server,
        "create-1",
        "project:project-1",
        None,
        draft(LocalStoryKind::Project, 1),
    )
    .await;
    assert!(matches!(
        created,
        LocalAgentIpcResponse::Story(ref record)
            if record.revision == 1 && record.draft.state["version"] == 1
    ));

    let duplicate = put(
        &server,
        "duplicate-1",
        "project:project-1",
        None,
        draft(LocalStoryKind::Project, 2),
    )
    .await;
    assert!(matches!(
        duplicate,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "story_revision_conflict"
    ));

    let updated = put(
        &server,
        "update-1",
        "project:project-1",
        Some(1),
        draft(LocalStoryKind::Project, 2),
    )
    .await;
    assert!(matches!(
        updated,
        LocalAgentIpcResponse::Story(ref record)
            if record.revision == 2 && record.draft.state["version"] == 2
    ));

    let stale = put(
        &server,
        "stale-1",
        "project:project-1",
        Some(1),
        draft(LocalStoryKind::Project, 3),
    )
    .await;
    assert!(matches!(
        stale,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "story_revision_conflict"
    ));
}

#[tokio::test]
async fn story_queries_are_owner_scoped_paginated_and_deletable() {
    let server = server().await;
    let _ = put(
        &server,
        "create-project",
        "project:project-1",
        None,
        draft(LocalStoryKind::Project, 1),
    )
    .await;
    let _ = put(
        &server,
        "create-run",
        "agent-run:run-1",
        None,
        draft(LocalStoryKind::AgentRun, 1),
    )
    .await;

    let first_page = server
        .handle_request(request(
            "alice",
            "list-1",
            LocalAgentCommand::ListStories(ListStoriesCommand {
                cursor: None,
                limit: 1,
            }),
        ))
        .await;
    let next_cursor = match first_page.response {
        LocalAgentIpcResponse::StoryRecords {
            records,
            next_cursor: Some(next_cursor),
        } => {
            assert_eq!(records.len(), 1);
            next_cursor
        }
        other => panic!("unexpected first page: {other:?}"),
    };
    let second_page = server
        .handle_request(request(
            "alice",
            "list-2",
            LocalAgentCommand::ListStories(ListStoriesCommand {
                cursor: Some(next_cursor),
                limit: 1,
            }),
        ))
        .await;
    let final_cursor = match second_page.response {
        LocalAgentIpcResponse::StoryRecords {
            records,
            next_cursor,
        } => {
            assert_eq!(records.len(), 1);
            next_cursor
        }
        other => panic!("unexpected second page: {other:?}"),
    };
    if let Some(final_cursor) = final_cursor {
        let terminal_page = server
            .handle_request(request(
                "alice",
                "list-3",
                LocalAgentCommand::ListStories(ListStoriesCommand {
                    cursor: Some(final_cursor),
                    limit: 1,
                }),
            ))
            .await;
        assert!(matches!(
            terminal_page.response,
            LocalAgentIpcResponse::StoryRecords { ref records, next_cursor: None }
                if records.is_empty()
        ));
    }

    let foreign = server
        .handle_request(request(
            "bob",
            "foreign-1",
            LocalAgentCommand::GetStory(GetStoryCommand {
                record_id: "project:project-1".to_string(),
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
            "delete-1",
            LocalAgentCommand::DeleteStory(DeleteStoryCommand {
                record_id: "project:project-1".to_string(),
                expected_revision: 1,
            }),
        ))
        .await;
    assert!(matches!(deleted.response, LocalAgentIpcResponse::Success));

    let missing = server
        .handle_request(request(
            "alice",
            "get-deleted",
            LocalAgentCommand::GetStory(GetStoryCommand {
                record_id: "project:project-1".to_string(),
            }),
        ))
        .await;
    assert!(matches!(
        missing.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "story_not_found"
    ));
}

#[tokio::test]
async fn story_kind_and_record_identity_mismatch_is_rejected_before_storage() {
    let server = server().await;
    let response = put(
        &server,
        "invalid-kind",
        "project:project-1",
        None,
        draft(LocalStoryKind::MediaBatch, 1),
    )
    .await;
    assert!(matches!(
        response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "invalid_ipc_request"
    ));
}
