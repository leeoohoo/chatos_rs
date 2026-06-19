use crate::models::memory_mapping_types::{MemoryProjectDto, SyncMemoryProjectRequestDto};
use crate::repositories::chatos_memory_mappings as mappings_repo;

use super::support::project_to_dto;

pub async fn sync_memory_project(
    payload: &SyncMemoryProjectRequestDto,
) -> Result<MemoryProjectDto, String> {
    let user_id = payload
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "user_id is required".to_string())?
        .to_string();
    let project_id = payload
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("0")
        .to_string();
    let project = mappings_repo::upsert_memory_project(mappings_repo::UpsertMemoryProjectInput {
        user_id,
        project_id,
        name: payload.name.clone().unwrap_or_else(|| {
            mappings_repo::default_project_name(payload.project_id.as_deref().unwrap_or("0"))
        }),
        root_path: payload.root_path.clone(),
        description: payload.description.clone(),
        status: payload.status.clone(),
        is_virtual: payload.is_virtual.map(|value| if value { 1 } else { 0 }),
    })
    .await?
    .ok_or_else(|| "sync memory project failed".to_string())?;
    Ok(project_to_dto(project))
}

pub async fn list_memory_projects(
    user_id: &str,
    status: Option<&str>,
    include_virtual: Option<bool>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectDto>, String> {
    let items = mappings_repo::list_memory_projects(
        user_id,
        status,
        include_virtual.unwrap_or(true),
        limit.unwrap_or(200),
        offset,
    )
    .await?;
    Ok(items.into_iter().map(project_to_dto).collect())
}
