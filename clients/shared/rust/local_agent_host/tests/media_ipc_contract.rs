// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalMediaIpcExecutor,
};
use chatos_local_agent_protocol::{
    DeleteMediaCommand, GetMediaCommand, ListMediaCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcRequest, LocalAgentIpcResponse, LocalMediaAsset, LocalMediaDraft, LocalMediaKind,
    LocalMediaStatus, PutMediaCommand, LOCAL_AGENT_PROTOCOL_VERSION,
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

fn draft(status: LocalMediaStatus, filename: Option<&str>) -> LocalMediaDraft {
    LocalMediaDraft {
        project_id: Some("project-1".to_string()),
        kind: LocalMediaKind::Image,
        status,
        prompt: "A quiet landscape".to_string(),
        model_name: "image-model".to_string(),
        generated_at: chrono::Utc::now(),
        assets: filename
            .map(|filename| {
                vec![LocalMediaAsset {
                    asset_id: filename.to_string(),
                    mime_type: "image/png".to_string(),
                    payload_reference: format!(
                        "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/media-1/{filename}"
                    ),
                    content_hash: format!("sha256:{}", "a".repeat(64)),
                    byte_count: 4,
                    revised_prompt: None,
                }]
            })
            .unwrap_or_default(),
    }
}

async fn server() -> LocalAgentIpcServer {
    let directory = tempfile::tempdir().unwrap().keep();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:media-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([83; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> = Arc::new(LocalMediaIpcExecutor::new(
        storage.clone(),
        scope(),
        "device-1",
        Arc::new(RejectTail),
    ));
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

#[tokio::test]
async fn media_status_and_assets_update_with_revision_cas() {
    let server = server().await;
    let created = server
        .handle_request(request(
            "alice",
            "create-1",
            LocalAgentCommand::PutMedia(PutMediaCommand {
                record_id: "media-1".to_string(),
                expected_revision: None,
                draft: draft(LocalMediaStatus::Pending, None),
            }),
        ))
        .await;
    assert!(matches!(
        created.response,
        LocalAgentIpcResponse::MediaMutation(ref result)
            if result.record.as_ref().is_some_and(|record|
                record.revision == 1 && record.draft.status == LocalMediaStatus::Pending)
    ));

    let completed = server
        .handle_request(request(
            "alice",
            "complete-1",
            LocalAgentCommand::PutMedia(PutMediaCommand {
                record_id: "media-1".to_string(),
                expected_revision: Some(1),
                draft: draft(LocalMediaStatus::Completed, Some("first.png")),
            }),
        ))
        .await;
    assert!(matches!(
        completed.response,
        LocalAgentIpcResponse::MediaMutation(ref result)
            if result.record.as_ref().is_some_and(|record|
                record.revision == 2 && record.draft.assets.len() == 1)
                && result.discarded_payload_references.is_empty()
    ));

    let stale = server
        .handle_request(request(
            "alice",
            "stale-1",
            LocalAgentCommand::PutMedia(PutMediaCommand {
                record_id: "media-1".to_string(),
                expected_revision: Some(1),
                draft: draft(LocalMediaStatus::Completed, Some("second.png")),
            }),
        ))
        .await;
    assert!(matches!(
        stale.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "media_revision_conflict"
    ));

    let replaced = server
        .handle_request(request(
            "alice",
            "replace-1",
            LocalAgentCommand::PutMedia(PutMediaCommand {
                record_id: "media-1".to_string(),
                expected_revision: Some(2),
                draft: draft(LocalMediaStatus::Completed, Some("second.png")),
            }),
        ))
        .await;
    assert!(matches!(
        replaced.response,
        LocalAgentIpcResponse::MediaMutation(ref result)
            if result.record.as_ref().is_some_and(|record| record.revision == 3)
                && result.discarded_payload_references == [
                    "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/media-1/first.png"
                ]
    ));
}

#[tokio::test]
async fn media_queries_are_owner_scoped_and_delete_returns_payload_references() {
    let server = server().await;
    let _ = server
        .handle_request(request(
            "alice",
            "create-1",
            LocalAgentCommand::PutMedia(PutMediaCommand {
                record_id: "media-1".to_string(),
                expected_revision: None,
                draft: draft(LocalMediaStatus::Completed, Some("first.png")),
            }),
        ))
        .await;

    let listed = server
        .handle_request(request(
            "alice",
            "list-1",
            LocalAgentCommand::ListMedia(ListMediaCommand {
                cursor: None,
                limit: 50,
            }),
        ))
        .await;
    assert!(matches!(
        listed.response,
        LocalAgentIpcResponse::MediaRecords { ref records, .. }
            if records.len() == 1 && records[0].record_id == "media-1"
    ));

    let foreign = server
        .handle_request(request(
            "bob",
            "foreign-1",
            LocalAgentCommand::GetMedia(GetMediaCommand {
                record_id: "media-1".to_string(),
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
            LocalAgentCommand::DeleteMedia(DeleteMediaCommand {
                record_id: "media-1".to_string(),
                expected_revision: 1,
            }),
        ))
        .await;
    assert!(matches!(
        deleted.response,
        LocalAgentIpcResponse::MediaMutation(ref result)
            if result.record.is_none()
                && result.discarded_payload_references == [
                    "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/media-1/first.png"
                ]
    ));
}
