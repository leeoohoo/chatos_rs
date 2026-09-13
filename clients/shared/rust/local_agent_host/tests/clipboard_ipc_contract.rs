// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalClipboardIpcExecutor,
};
use chatos_local_agent_protocol::{
    DeleteClipboardCommand, GetClipboardCommand, ListClipboardCommand, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse, LocalClipboardDraft,
    LocalClipboardKind, SetClipboardPinnedCommand, StoreClipboardCommand,
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

fn draft(hash_character: char, payload_reference: &str, source: &str) -> LocalClipboardDraft {
    LocalClipboardDraft {
        kind: LocalClipboardKind::Text,
        mime_type: "text/plain".to_string(),
        content_hash: format!("sha256:{}", hash_character.to_string().repeat(64)),
        text_preview: Some("Clipboard preview".to_string()),
        source_bundle_id: Some(source.to_string()),
        payload_reference: payload_reference.to_string(),
        byte_count: 17,
        pasteboard_type: None,
    }
}

async fn server() -> LocalAgentIpcServer {
    let directory = tempfile::tempdir().unwrap().keep();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:clipboard-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([79; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> = Arc::new(
        LocalClipboardIpcExecutor::new(storage.clone(), scope(), "device-1", Arc::new(RejectTail)),
    );
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

#[tokio::test]
async fn clipboard_deduplication_and_pinning_are_atomic_and_revision_bound() {
    let server = server().await;
    let first = server
        .handle_request(request(
            "alice",
            "store-1",
            LocalAgentCommand::StoreClipboard(StoreClipboardCommand {
                entry_id: "entry-1".to_string(),
                draft: draft('a', "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/entry-1.txt", "app.first"),
            }),
        ))
        .await;
    let LocalAgentIpcResponse::ClipboardMutation(first) = first.response else {
        panic!("expected clipboard mutation");
    };
    let first = first.entry.unwrap();
    assert_eq!(first.entry_id, "entry-1");
    assert_eq!(first.revision, 1);

    let duplicate = server
        .handle_request(request(
            "alice",
            "store-2",
            LocalAgentCommand::StoreClipboard(StoreClipboardCommand {
                entry_id: "entry-2".to_string(),
                draft: draft('a', "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/entry-2.txt", "app.second"),
            }),
        ))
        .await;
    let LocalAgentIpcResponse::ClipboardMutation(duplicate) = duplicate.response else {
        panic!("expected clipboard mutation");
    };
    assert_eq!(
        duplicate.discarded_payload_references,
        ["Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/entry-2.txt"]
    );
    let duplicate = duplicate.entry.unwrap();
    assert_eq!(duplicate.entry_id, "entry-1");
    assert_eq!(duplicate.revision, 2);
    assert_eq!(
        duplicate.draft.source_bundle_id.as_deref(),
        Some("app.second")
    );

    let stale = server
        .handle_request(request(
            "alice",
            "pin-stale",
            LocalAgentCommand::SetClipboardPinned(SetClipboardPinnedCommand {
                entry_id: "entry-1".to_string(),
                expected_revision: 1,
                is_pinned: true,
            }),
        ))
        .await;
    assert!(matches!(
        stale.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "clipboard_revision_conflict"
    ));

    let pinned = server
        .handle_request(request(
            "alice",
            "pin-1",
            LocalAgentCommand::SetClipboardPinned(SetClipboardPinnedCommand {
                entry_id: "entry-1".to_string(),
                expected_revision: 2,
                is_pinned: true,
            }),
        ))
        .await;
    assert!(matches!(
        pinned.response,
        LocalAgentIpcResponse::ClipboardMutation(ref result)
            if result.entry.as_ref().is_some_and(|entry| entry.is_pinned && entry.revision == 3)
    ));

    let listed = server
        .handle_request(request(
            "alice",
            "list-1",
            LocalAgentCommand::ListClipboard(ListClipboardCommand {
                cursor: None,
                limit: 50,
            }),
        ))
        .await;
    assert!(matches!(
        listed.response,
        LocalAgentIpcResponse::ClipboardRecords { ref entries, .. }
            if entries.len() == 1 && entries[0].is_pinned
    ));
}

#[tokio::test]
async fn clipboard_queries_are_owner_scoped_and_deletion_returns_only_local_references() {
    let server = server().await;
    let _ = server
        .handle_request(request(
            "alice",
            "store-1",
            LocalAgentCommand::StoreClipboard(StoreClipboardCommand {
                entry_id: "entry-1".to_string(),
                draft: draft('b', "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/entry-1.txt", "app.first"),
            }),
        ))
        .await;

    let foreign = server
        .handle_request(request(
            "bob",
            "foreign-1",
            LocalAgentCommand::GetClipboard(GetClipboardCommand {
                entry_id: "entry-1".to_string(),
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
            LocalAgentCommand::DeleteClipboard(DeleteClipboardCommand {
                entry_id: "entry-1".to_string(),
                expected_revision: 1,
            }),
        ))
        .await;
    assert!(matches!(
        deleted.response,
        LocalAgentIpcResponse::ClipboardMutation(ref result)
            if result.entry.is_none()
                && result.discarded_payload_references == ["Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/entry-1.txt"]
    ));

    let missing = server
        .handle_request(request(
            "alice",
            "get-missing",
            LocalAgentCommand::GetClipboard(GetClipboardCommand {
                entry_id: "entry-1".to_string(),
            }),
        ))
        .await;
    assert!(matches!(
        missing.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "clipboard_not_found"
    ));

    for (id, hash, file) in [
        ("entry-2", 'c', "entry-2.txt"),
        ("entry-3", 'd', "entry-3.txt"),
    ] {
        let _ = server
            .handle_request(request(
                "alice",
                id,
                LocalAgentCommand::StoreClipboard(StoreClipboardCommand {
                    entry_id: id.to_string(),
                    draft: draft(
                        hash,
                        &format!(
                            "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/{file}"
                        ),
                        "app.first",
                    ),
                }),
            ))
            .await;
    }
    for (id, payload_reference) in [
        (
            "entry-2",
            "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/entry-2.txt",
        ),
        (
            "entry-3",
            "Payloads/2bd806c97f0e00af1a1fc3328fa763a9269723c8db8fac4f93af71db186d6e90/entry-3.txt",
        ),
    ] {
        let deleted = server
            .handle_request(request(
                "alice",
                &format!("delete-{id}"),
                LocalAgentCommand::DeleteClipboard(DeleteClipboardCommand {
                    entry_id: id.to_string(),
                    expected_revision: 1,
                }),
            ))
            .await;
        assert!(matches!(
            deleted.response,
            LocalAgentIpcResponse::ClipboardMutation(ref result)
                if result.entry.is_none()
                    && result.discarded_payload_references == [payload_reference]
        ));
    }
}
