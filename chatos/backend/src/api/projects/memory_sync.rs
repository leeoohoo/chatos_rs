// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::memory_mapping_types::SyncMemoryProjectRequestDto;
use crate::models::project::Project;
use crate::services::chatos_memory_mappings;

async fn sync_memory_project_state(project: &Project, status: &str) -> Result<(), String> {
    chatos_memory_mappings::sync_memory_project(&SyncMemoryProjectRequestDto {
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

pub(crate) async fn sync_active_project(project: &Project) -> Result<(), String> {
    sync_memory_project_state(project, "active").await
}

pub(super) async fn sync_archived_project(project: &Project) -> Result<(), String> {
    sync_memory_project_state(project, "archived").await
}
