// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalProjectIpcExecutor,
};
use chatos_local_agent_protocol::{
    CreateProjectCommand, GetProjectCommand, ListProjectsCommand, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse, LocalProjectDraft,
    LocalProjectStatus, UpdateProjectCommand, LOCAL_AGENT_PROTOCOL_VERSION,
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

fn draft(name: &str) -> LocalProjectDraft {
    LocalProjectDraft {
        name: name.to_string(),
        description: "Visual editor".to_string(),
        root_path: "/apps/editor".to_string(),
    }
}

async fn server() -> LocalAgentIpcServer {
    let directory = tempfile::tempdir().unwrap().keep();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:project-ipc").unwrap(),
            },
            &StorageEncryptionKey::new([77; 32]),
        )
        .await
        .unwrap(),
    );
    let executor: Arc<dyn LocalAgentIpcMutationExecutor> = Arc::new(LocalProjectIpcExecutor::new(
        storage.clone(),
        scope(),
        "device-1",
        Arc::new(RejectTail),
    ));
    LocalAgentIpcServer::new(storage, scope(), executor).unwrap()
}

#[tokio::test]
async fn typed_project_crud_is_owner_scoped_and_revision_bound() {
    let server = server().await;
    let created = server
        .handle_request(request(
            "alice",
            "create-1",
            LocalAgentCommand::CreateProject(CreateProjectCommand {
                project_id: "project-1".to_string(),
                draft: draft("Editor"),
            }),
        ))
        .await;
    let LocalAgentIpcResponse::Project(created) = created.response else {
        panic!("expected a project response");
    };
    assert_eq!(created.owner_user_id, "alice");
    assert_eq!(created.project_id, "project-1");
    assert_eq!(created.revision, 1);
    assert_eq!(created.status, LocalProjectStatus::Active);

    let foreign = server
        .handle_request(request(
            "bob",
            "get-foreign",
            LocalAgentCommand::GetProject(GetProjectCommand {
                project_id: "project-1".to_string(),
            }),
        ))
        .await;
    assert!(matches!(
        foreign.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "owner_scope_mismatch"
    ));

    let updated = server
        .handle_request(request(
            "alice",
            "update-1",
            LocalAgentCommand::UpdateProject(UpdateProjectCommand {
                project_id: "project-1".to_string(),
                expected_revision: 1,
                draft: draft("Renamed"),
                status: LocalProjectStatus::Archived,
            }),
        ))
        .await;
    let LocalAgentIpcResponse::Project(updated) = updated.response else {
        panic!("expected an updated project response");
    };
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.status, LocalProjectStatus::Archived);

    let stale = server
        .handle_request(request(
            "alice",
            "stale-1",
            LocalAgentCommand::UpdateProject(UpdateProjectCommand {
                project_id: "project-1".to_string(),
                expected_revision: 1,
                draft: draft("Stale"),
                status: LocalProjectStatus::Active,
            }),
        ))
        .await;
    assert!(matches!(
        stale.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. })
            if code == "project_revision_conflict"
    ));

    let active = server
        .handle_request(request(
            "alice",
            "list-active",
            LocalAgentCommand::ListProjects(ListProjectsCommand {
                cursor: None,
                limit: 50,
                include_inactive: false,
            }),
        ))
        .await;
    assert!(matches!(
        active.response,
        LocalAgentIpcResponse::Projects { ref projects, .. } if projects.is_empty()
    ));

    let all = server
        .handle_request(request(
            "alice",
            "list-all",
            LocalAgentCommand::ListProjects(ListProjectsCommand {
                cursor: None,
                limit: 50,
                include_inactive: true,
            }),
        ))
        .await;
    assert!(matches!(
        all.response,
        LocalAgentIpcResponse::Projects { ref projects, .. }
            if projects.len() == 1 && projects[0].revision == 2
    ));
}

#[tokio::test]
async fn removed_project_cannot_be_resurrected() {
    let server = server().await;
    let _ = server
        .handle_request(request(
            "alice",
            "create-1",
            LocalAgentCommand::CreateProject(CreateProjectCommand {
                project_id: "project-1".to_string(),
                draft: draft("Editor"),
            }),
        ))
        .await;
    let _ = server
        .handle_request(request(
            "alice",
            "remove-1",
            LocalAgentCommand::UpdateProject(UpdateProjectCommand {
                project_id: "project-1".to_string(),
                expected_revision: 1,
                draft: draft("Editor"),
                status: LocalProjectStatus::Removed,
            }),
        ))
        .await;
    let resurrect = server
        .handle_request(request(
            "alice",
            "resurrect-1",
            LocalAgentCommand::UpdateProject(UpdateProjectCommand {
                project_id: "project-1".to_string(),
                expected_revision: 2,
                draft: draft("Editor"),
                status: LocalProjectStatus::Active,
            }),
        ))
        .await;
    assert!(matches!(
        resurrect.response,
        LocalAgentIpcResponse::Error(LocalAgentIpcError { ref code, .. }) if code == "project_invalid"
    ));
}
