// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalNotepadIpcExecutor,
};
use chatos_local_agent_protocol::{
    DeleteNotepadCommand, DeleteNotepadFolderCommand, GetNotepadCommand, ListNotepadCommand,
    LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse,
    LocalNotepadDraft, LocalNotepadKind, PutNotepadCommand, RenameNotepadFolderCommand,
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

fn folder(path: &str) -> LocalNotepadDraft {
    LocalNotepadDraft {
        kind: LocalNotepadKind::Folder,
        folder: path.to_string(),
        title: String::new(),
        content: String::new(),
        tags: Vec::new(),
    }
}

fn note(folder: &str, title: &str) -> LocalNotepadDraft {
    LocalNotepadDraft {
        kind: LocalNotepadKind::Note,
        folder: folder.to_string(),
        title: title.to_string(),
        content: format!("{title} content"),
        tags: vec!["design".to_string()],
    }
}

async fn server() -> LocalAgentIpcServer {
    let directory = tempfile::tempdir().unwrap().keep();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:notepad-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([97; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> = Arc::new(LocalNotepadIpcExecutor::new(
        storage.clone(),
        scope(),
        "device-1",
        Arc::new(RejectTail),
    ));
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

async fn execute(
    server: &LocalAgentIpcServer,
    request_id: &str,
    command: LocalAgentCommand,
) -> LocalAgentIpcResponse {
    server
        .handle_request(request("alice", request_id, command))
        .await
        .response
}

async fn put(
    server: &LocalAgentIpcServer,
    request_id: &str,
    record_id: &str,
    expected_revision: Option<u64>,
    draft: LocalNotepadDraft,
) -> LocalAgentIpcResponse {
    execute(
        server,
        request_id,
        LocalAgentCommand::PutNotepad(PutNotepadCommand {
            record_id: record_id.to_string(),
            expected_revision,
            draft,
        }),
    )
    .await
}

async fn get(server: &LocalAgentIpcServer, record_id: &str) -> LocalAgentIpcResponse {
    execute(
        server,
        &format!("get-{record_id}"),
        LocalAgentCommand::GetNotepad(GetNotepadCommand {
            record_id: record_id.to_string(),
        }),
    )
    .await
}

async fn list_all(
    server: &LocalAgentIpcServer,
) -> Vec<chatos_local_agent_protocol::LocalNotepadSnapshot> {
    let mut records = Vec::new();
    let mut cursor = None;
    loop {
        let response = execute(
            server,
            "list-all",
            LocalAgentCommand::ListNotepad(ListNotepadCommand {
                cursor: cursor.clone(),
                limit: 2,
            }),
        )
        .await;
        match response {
            LocalAgentIpcResponse::NotepadRecords {
                records: page,
                next_cursor,
            } => {
                records.extend(page);
                if next_cursor.is_none() {
                    return records;
                }
                cursor = next_cursor;
            }
            other => panic!("unexpected notepad list response: {other:?}"),
        }
    }
}

#[tokio::test]
async fn notepad_create_update_query_and_delete_require_revision_cas() {
    let server = server().await;
    let folder_created = put(
        &server,
        "folder-create",
        "folder:design",
        None,
        folder("design"),
    )
    .await;
    assert!(matches!(
        folder_created,
        LocalAgentIpcResponse::Notepad(ref snapshot)
            if snapshot.revision == 1 && snapshot.draft.kind == LocalNotepadKind::Folder
    ));

    let note_created = put(
        &server,
        "note-create",
        "note:hero",
        None,
        note("design", "Hero"),
    )
    .await;
    assert!(matches!(
        note_created,
        LocalAgentIpcResponse::Notepad(ref snapshot)
            if snapshot.revision == 1 && snapshot.draft.content == "Hero content"
    ));

    let duplicate = put(
        &server,
        "note-duplicate",
        "note:hero",
        None,
        note("design", "Duplicate"),
    )
    .await;
    assert!(matches!(
        duplicate,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "notepad_revision_conflict"
    ));

    let updated = put(
        &server,
        "note-update",
        "note:hero",
        Some(1),
        note("design", "Updated hero"),
    )
    .await;
    assert!(matches!(
        updated,
        LocalAgentIpcResponse::Notepad(ref snapshot)
            if snapshot.revision == 2 && snapshot.draft.title == "Updated hero"
    ));

    let stale = put(
        &server,
        "note-stale",
        "note:hero",
        Some(1),
        note("design", "Stale"),
    )
    .await;
    assert!(matches!(
        stale,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "notepad_revision_conflict"
    ));

    let first_page = execute(
        &server,
        "list-first",
        LocalAgentCommand::ListNotepad(ListNotepadCommand {
            cursor: None,
            limit: 1,
        }),
    )
    .await;
    assert!(matches!(
        first_page,
        LocalAgentIpcResponse::NotepadRecords { ref records, next_cursor: Some(_) }
            if records.len() == 1
    ));

    let fetched = get(&server, "note:hero").await;
    assert!(matches!(
        fetched,
        LocalAgentIpcResponse::Notepad(ref snapshot)
            if snapshot.revision == 2 && snapshot.draft.title == "Updated hero"
    ));

    let stale_delete = execute(
        &server,
        "delete-stale",
        LocalAgentCommand::DeleteNotepad(DeleteNotepadCommand {
            record_id: "note:hero".to_string(),
            expected_revision: 1,
        }),
    )
    .await;
    assert!(matches!(
        stale_delete,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "notepad_revision_conflict"
    ));

    let deleted = execute(
        &server,
        "delete-current",
        LocalAgentCommand::DeleteNotepad(DeleteNotepadCommand {
            record_id: "note:hero".to_string(),
            expected_revision: 2,
        }),
    )
    .await;
    assert!(matches!(deleted, LocalAgentIpcResponse::Success));
    assert!(matches!(
        get(&server, "note:hero").await,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "notepad_not_found"
    ));
}

#[tokio::test]
async fn notepad_owner_and_kind_identity_are_rejected_before_storage() {
    let server = server().await;
    let foreign = server
        .handle_request(request(
            "bob",
            "foreign",
            LocalAgentCommand::ListNotepad(ListNotepadCommand {
                cursor: None,
                limit: 20,
            }),
        ))
        .await;
    assert!(matches!(
        foreign.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "owner_scope_mismatch"
    ));

    let mismatch = put(
        &server,
        "kind-mismatch",
        "folder:wrong",
        None,
        note("design", "Wrong identity"),
    )
    .await;
    assert!(matches!(
        mismatch,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "invalid_ipc_request"
    ));
    assert!(list_all(&server).await.is_empty());
}

#[tokio::test]
async fn folder_rename_updates_the_entire_tree_atomically() {
    let server = server().await;
    let _ = put(&server, "root", "folder:design", None, folder("design")).await;
    let _ = put(
        &server,
        "child",
        "folder:research",
        None,
        folder("design/research"),
    )
    .await;
    let _ = put(
        &server,
        "root-note",
        "note:root",
        None,
        note("design", "Root"),
    )
    .await;
    let _ = put(
        &server,
        "child-note",
        "note:child",
        None,
        note("design/research", "Child"),
    )
    .await;
    let _ = put(
        &server,
        "outside",
        "note:outside",
        None,
        note("other", "Outside"),
    )
    .await;

    let renamed = execute(
        &server,
        "rename",
        LocalAgentCommand::RenameNotepadFolder(RenameNotepadFolderCommand {
            folder: "design".to_string(),
            replacement: "visual".to_string(),
        }),
    )
    .await;
    assert!(matches!(renamed, LocalAgentIpcResponse::Success));

    let records = list_all(&server).await;
    let paths = records
        .iter()
        .map(|record| {
            (
                record.record_id.as_str(),
                record.draft.folder.as_str(),
                record.revision,
            )
        })
        .collect::<Vec<_>>();
    assert!(paths.contains(&("folder:design", "visual", 2)));
    assert!(paths.contains(&("folder:research", "visual/research", 2)));
    assert!(paths.contains(&("note:root", "visual", 2)));
    assert!(paths.contains(&("note:child", "visual/research", 2)));
    assert!(paths.contains(&("note:outside", "other", 1)));
}

#[tokio::test]
async fn folder_rename_conflict_rolls_back_every_record() {
    let server = server().await;
    let _ = put(&server, "source", "folder:source", None, folder("source")).await;
    let _ = put(
        &server,
        "source-note",
        "note:source",
        None,
        note("source", "Source"),
    )
    .await;
    let _ = put(&server, "target", "folder:target", None, folder("target")).await;

    let response = execute(
        &server,
        "rename-conflict",
        LocalAgentCommand::RenameNotepadFolder(RenameNotepadFolderCommand {
            folder: "source".to_string(),
            replacement: "target".to_string(),
        }),
    )
    .await;
    assert!(matches!(
        response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "notepad_revision_conflict"
    ));

    let records = list_all(&server).await;
    assert!(records.iter().any(|record| {
        record.record_id == "folder:source"
            && record.draft.folder == "source"
            && record.revision == 1
    }));
    assert!(records.iter().any(|record| {
        record.record_id == "note:source" && record.draft.folder == "source" && record.revision == 1
    }));
    assert!(records.iter().any(|record| {
        record.record_id == "folder:target"
            && record.draft.folder == "target"
            && record.revision == 1
    }));
}

#[tokio::test]
async fn folder_delete_is_non_recursive_safe_and_recursive_atomic() {
    let server = server().await;
    let _ = put(&server, "root", "folder:root", None, folder("root")).await;
    let _ = put(&server, "child", "folder:child", None, folder("root/child")).await;
    let _ = put(
        &server,
        "note",
        "note:child",
        None,
        note("root/child", "Child"),
    )
    .await;
    let _ = put(
        &server,
        "outside",
        "note:outside",
        None,
        note("outside", "Outside"),
    )
    .await;

    let refused = execute(
        &server,
        "delete-non-recursive",
        LocalAgentCommand::DeleteNotepadFolder(DeleteNotepadFolderCommand {
            folder: "root".to_string(),
            recursive: false,
        }),
    )
    .await;
    assert!(matches!(
        refused,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "notepad_invalid"
    ));
    assert_eq!(list_all(&server).await.len(), 4);

    let deleted = execute(
        &server,
        "delete-recursive",
        LocalAgentCommand::DeleteNotepadFolder(DeleteNotepadFolderCommand {
            folder: "root".to_string(),
            recursive: true,
        }),
    )
    .await;
    assert!(matches!(deleted, LocalAgentIpcResponse::Success));

    let remaining = list_all(&server).await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].record_id, "note:outside");
}
