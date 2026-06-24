use serde_json::Value;

use crate::models::memory_mapping_types::{MemoryAgentRecallDto, MemoryProjectMemoryDto};
use crate::models::project::{normalize_project_id, PUBLIC_PROJECT_ID};
use crate::repositories::chatos_memory_mappings as mappings_repo;
use crate::services::chatos_memory_engine;

use super::support::max_timestamp_opt;

pub async fn list_contact_project_memories(
    contact_id: &str,
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let contact = mappings_repo::get_contact_by_id(contact_id)
        .await?
        .ok_or_else(|| "contact not found".to_string())?;
    let project_id = normalize_project_id(project_id);
    chatos_memory_engine::list_contact_project_memories(
        contact.user_id.as_str(),
        contact.id.as_str(),
        project_id.as_str(),
        limit,
        offset,
    )
    .await
}

pub async fn list_contact_project_memories_by_contact(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let contact = mappings_repo::get_contact_by_id(contact_id)
        .await?
        .ok_or_else(|| "contact not found".to_string())?;
    let links = mappings_repo::list_project_agent_links_by_contact(
        contact.user_id.as_str(),
        contact.id.as_str(),
        Some("active"),
        500,
        0,
    )
    .await?;
    let mut project_ids = links
        .into_iter()
        .map(|item| normalize_project_id(item.project_id.as_str()))
        .collect::<Vec<_>>();
    project_ids.sort();
    project_ids.dedup();
    chatos_memory_engine::list_contact_project_memories_by_contact(
        contact.user_id.as_str(),
        contact.id.as_str(),
        project_ids.as_slice(),
        limit,
        offset,
    )
    .await
}

pub async fn list_contact_projects(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<Value>, String> {
    let contact = mappings_repo::get_contact_by_id(contact_id)
        .await?
        .ok_or_else(|| "contact not found".to_string())?;
    let links = mappings_repo::list_project_agent_links_by_contact(
        contact.user_id.as_str(),
        contact.id.as_str(),
        Some("active"),
        limit.unwrap_or(200),
        offset,
    )
    .await?;
    let mut project_ids = links
        .iter()
        .map(|item| item.project_id.clone())
        .collect::<Vec<_>>();
    project_ids.sort();
    project_ids.dedup();
    let projects =
        mappings_repo::list_projects_by_ids(contact.user_id.as_str(), project_ids.as_slice())
            .await?;
    let project_map = projects
        .into_iter()
        .map(|item| (item.project_id.clone(), item))
        .collect::<std::collections::HashMap<_, _>>();
    let memories = chatos_memory_engine::list_contact_project_memories_by_contact(
        contact.user_id.as_str(),
        contact.id.as_str(),
        project_ids.as_slice(),
        Some(2000),
        0,
    )
    .await?;
    let latest_memory_by_project = memories.into_iter().fold(
        std::collections::HashMap::<String, MemoryProjectMemoryDto>::new(),
        |mut acc, item| {
            let replace = acc
                .get(item.project_id.as_str())
                .map(|existing| existing.updated_at.as_str() <= item.updated_at.as_str())
                .unwrap_or(true);
            if replace {
                acc.insert(item.project_id.clone(), item);
            }
            acc
        },
    );
    let mut out = Vec::new();
    for project_id in project_ids {
        let project = project_map.get(project_id.as_str());
        let latest_memory = latest_memory_by_project.get(project_id.as_str());
        let updated_at = max_timestamp_opt(
            project.map(|item| item.updated_at.as_str()),
            latest_memory.map(|item| item.updated_at.as_str()),
        );
        out.push(serde_json::json!({
            "project_id": project_id,
            "project_name": project
                .map(|item| item.name.clone())
                .unwrap_or_else(|| mappings_repo::default_project_name(project_id.as_str())),
            "project_root": project.and_then(|item| item.root_path.clone()),
            "status": project
                .map(|item| item.status.clone())
                .unwrap_or_else(|| "active".to_string()),
            "is_virtual": project
                .map(|item| item.is_virtual)
                .unwrap_or_else(|| if project_id == PUBLIC_PROJECT_ID { 1 } else { 0 }),
            "has_memory": latest_memory.is_some(),
            "memory_version": latest_memory.map(|item| item.memory_version).unwrap_or(0),
            "last_source_at": latest_memory.and_then(|item| item.last_source_at.clone()),
            "updated_at": updated_at,
        }));
    }
    Ok(out)
}

pub async fn list_contact_agent_recalls(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryAgentRecallDto>, String> {
    let contact = mappings_repo::get_contact_by_id(contact_id)
        .await?
        .ok_or_else(|| "contact not found".to_string())?;
    chatos_memory_engine::list_contact_agent_recalls(
        contact.user_id.as_str(),
        contact.agent_id.as_str(),
        limit,
        offset,
    )
    .await
}
