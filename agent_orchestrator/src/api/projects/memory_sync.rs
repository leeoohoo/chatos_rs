use crate::models::project::Project;
use crate::services::memory_server_client;

async fn sync_memory_project_state(project: &Project, status: &str) -> Result<(), String> {
    memory_server_client::sync_memory_project(&memory_server_client::SyncMemoryProjectRequestDto {
        user_id: project.user_id.clone(),
        project_id: Some(project.id.clone()),
        name: Some(project.name.clone()),
        root_path: Some(project.root_path.clone()),
        description: project.description.clone(),
        status: Some(status.to_string()),
        is_virtual: Some(false),
    })
    .await
    .map(|_| ())
}

pub(super) async fn sync_active_project(project: &Project) -> Result<(), String> {
    sync_memory_project_state(project, "active").await
}

pub(super) async fn sync_archived_project(project: &Project) -> Result<(), String> {
    sync_memory_project_state(project, "archived").await
}
