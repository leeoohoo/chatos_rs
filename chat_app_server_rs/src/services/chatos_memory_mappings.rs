use serde_json::Value;

use crate::models::memory_mapping::{ChatosContact, ChatosMemoryProject, ChatosProjectAgentLink};
use crate::models::memory_mapping_types::{
    CreateMemoryContactRequestDto, CreateMemoryContactResponseDto, MemoryAgentRecallDto,
    MemoryContactDto, MemoryProjectAgentLinkDto, MemoryProjectContactDto, MemoryProjectDto,
    MemoryProjectMemoryDto, SyncMemoryProjectRequestDto, SyncProjectAgentLinkRequestDto,
    UpdateContactTaskRunnerConfigRequestDto,
};
use crate::repositories::chatos_memory_mappings as mappings_repo;
use crate::repositories::projects;
use crate::services::chatos_memory_engine;

#[derive(Debug, Clone)]
pub struct ContactTaskRunnerRuntimeConfig {
    pub contact_id: String,
    pub base_url: String,
    pub username: String,
    pub password: String,
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub async fn list_memory_contacts(
    user_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryContactDto>, String> {
    let user_id = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "user_id is required".to_string())?;
    let items =
        mappings_repo::list_contacts(user_id, Some("active"), limit.unwrap_or(200), offset).await?;
    Ok(items.into_iter().map(contact_to_dto).collect())
}

pub async fn get_memory_contact(contact_id: &str) -> Result<Option<MemoryContactDto>, String> {
    Ok(mappings_repo::get_contact_by_id(contact_id)
        .await?
        .map(contact_to_dto))
}

pub async fn get_contact_task_runner_runtime_config(
    user_id: Option<&str>,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<Option<ContactTaskRunnerRuntimeConfig>, String> {
    let user_id = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "user_id is required".to_string())?;

    let contact =
        if let Some(contact_id) = contact_id.map(str::trim).filter(|value| !value.is_empty()) {
            mappings_repo::get_contact_by_id(contact_id).await?
        } else if let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) {
            mappings_repo::get_contact_by_user_and_agent(user_id, agent_id).await?
        } else {
            None
        };

    let Some(contact) = contact else {
        return Ok(None);
    };
    if contact.user_id != user_id || !contact.task_runner_enabled {
        return Ok(None);
    }
    let Some(base_url) = normalize_non_empty(contact.task_runner_base_url.as_deref()) else {
        return Ok(None);
    };
    let Some(username) = normalize_non_empty(contact.task_runner_username.as_deref()) else {
        return Ok(None);
    };
    let Some(password) = normalize_non_empty(contact.task_runner_password.as_deref()) else {
        return Ok(None);
    };

    Ok(Some(ContactTaskRunnerRuntimeConfig {
        contact_id: contact.id,
        base_url,
        username,
        password,
    }))
}

pub async fn create_memory_contact(
    payload: &CreateMemoryContactRequestDto,
) -> Result<CreateMemoryContactResponseDto, String> {
    let user_id = payload
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "user_id is required".to_string())?;
    let agent_id = payload.agent_id.trim().to_string();
    if agent_id.is_empty() {
        return Err("agent_id is required".to_string());
    }
    let (contact, created) = mappings_repo::create_contact_idempotent(
        user_id,
        agent_id.as_str(),
        payload.agent_name_snapshot.clone(),
    )
    .await?;
    Ok(CreateMemoryContactResponseDto {
        created,
        contact: contact_to_dto(contact),
    })
}

pub async fn delete_memory_contact(contact_id: &str) -> Result<bool, String> {
    mappings_repo::delete_contact_by_id(contact_id).await
}

pub async fn update_contact_task_runner_config(
    contact_id: &str,
    payload: &UpdateContactTaskRunnerConfigRequestDto,
) -> Result<Option<MemoryContactDto>, String> {
    let contact = mappings_repo::update_contact_task_runner_config(
        contact_id,
        mappings_repo::UpdateContactTaskRunnerConfigInput {
            enabled: payload.enabled,
            base_url: payload.base_url.clone(),
            username: payload.username.clone(),
            password: payload.password.clone(),
            clear_password: payload.clear_password.unwrap_or(false),
        },
    )
    .await?;
    Ok(contact.map(contact_to_dto))
}

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

pub async fn sync_project_agent_link(
    payload: &SyncProjectAgentLinkRequestDto,
) -> Result<MemoryProjectAgentLinkDto, String> {
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
    let agent_id = payload
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "agent_id is required".to_string())?
        .to_string();
    let contact_id = payload
        .contact_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "contact_id is required".to_string())?
        .to_string();
    let status = payload
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("active");
    if status != "active" {
        return Err("project contact links are deleted instead of archived".to_string());
    }

    if mappings_repo::get_project_by_user_and_project_id(user_id.as_str(), project_id.as_str())
        .await?
        .is_none()
    {
        let _ = mappings_repo::upsert_memory_project(mappings_repo::UpsertMemoryProjectInput {
            user_id: user_id.clone(),
            project_id: project_id.clone(),
            name: mappings_repo::default_project_name(project_id.as_str()),
            root_path: None,
            description: None,
            status: Some("active".to_string()),
            is_virtual: Some(if project_id == "0" { 1 } else { 0 }),
        })
        .await?;
    }

    let link =
        mappings_repo::upsert_project_agent_link(mappings_repo::UpsertProjectAgentLinkInput {
            user_id,
            project_id,
            agent_id,
            contact_id: Some(contact_id),
            latest_session_id: payload.session_id.clone(),
            last_message_at: payload.last_message_at.clone(),
            status: payload.status.clone(),
        })
        .await?
        .ok_or_else(|| "sync project-agent link failed".to_string())?;
    Ok(project_agent_link_to_dto(link))
}

pub async fn touch_current_project_contact_session(
    user_id: &str,
    project_id: &str,
    agent_id: &str,
    contact_id: &str,
    session_id: &str,
    last_message_at: &str,
) -> Result<bool, String> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Ok(false);
    }
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Ok(false);
    }
    let contact_id = contact_id.trim();
    if contact_id.is_empty() {
        return Ok(false);
    }
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Ok(false);
    }
    let project_id = project_id.trim();
    let project_id = if project_id.is_empty() {
        "0"
    } else {
        project_id
    };

    let updated = mappings_repo::touch_project_agent_link_session(
        mappings_repo::TouchProjectAgentLinkSessionInput {
            user_id: user_id.to_string(),
            project_id: project_id.to_string(),
            agent_id: agent_id.to_string(),
            contact_id: contact_id.to_string(),
            latest_session_id: session_id.to_string(),
            last_message_at: last_message_at.to_string(),
        },
    )
    .await?;
    Ok(updated.is_some())
}

pub async fn delete_project_contact_link(
    user_id: &str,
    project_id: &str,
    contact_id: &str,
) -> Result<bool, String> {
    let user_id = user_id.trim();
    let contact_id = contact_id.trim();
    if user_id.is_empty() || contact_id.is_empty() {
        return Ok(false);
    }
    let project_id = project_id.trim();
    let project_id = if project_id.is_empty() {
        "0"
    } else {
        project_id
    };
    mappings_repo::delete_project_agent_link(user_id, project_id, Some(contact_id)).await
}

pub async fn list_project_contacts(
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectContactDto>, String> {
    let owner = projects::get_project_by_id(project_id)
        .await?
        .ok_or_else(|| "project not found".to_string())?;
    let owner_user_id = owner
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "project owner is missing".to_string())?;

    let links = mappings_repo::list_project_agent_links_by_project(
        owner_user_id,
        project_id,
        Some("active"),
        limit.unwrap_or(200),
        offset,
    )
    .await;
    let links = match links {
        Ok(items) => items,
        Err(_) => Vec::new(),
    };
    if links.is_empty() {
        return Ok(Vec::new());
    }

    let user_id = links
        .first()
        .map(|item| item.user_id.clone())
        .unwrap_or_default();
    let contact_ids = links
        .iter()
        .filter_map(|item| item.contact_id.clone())
        .collect::<Vec<_>>();
    let contacts = mappings_repo::list_contacts_by_ids(
        user_id.as_str(),
        contact_ids.as_slice(),
        Some("active"),
    )
    .await?;
    let contact_map = contacts
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect::<std::collections::HashMap<_, _>>();

    let mut out = Vec::new();
    for link in links {
        let Some(contact_id) = link.contact_id.clone() else {
            continue;
        };
        let Some(contact) = contact_map.get(contact_id.as_str()) else {
            continue;
        };
        out.push(MemoryProjectContactDto {
            project_id: link.project_id.clone(),
            contact_id: contact.id.clone(),
            agent_id: contact.agent_id.clone(),
            agent_name_snapshot: contact.agent_name_snapshot.clone(),
            contact_status: contact.status.clone(),
            link_status: link.status.clone(),
            latest_session_id: link.latest_session_id.clone(),
            last_bound_at: Some(link.last_bound_at.clone()),
            last_message_at: link.last_message_at.clone(),
            created_at: contact.created_at.clone(),
            updated_at: max_timestamp(contact.updated_at.as_str(), link.updated_at.as_str()),
        });
    }
    Ok(out)
}

pub async fn list_contact_project_memories(
    contact_id: &str,
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let contact = mappings_repo::get_contact_by_id(contact_id)
        .await?
        .ok_or_else(|| "contact not found".to_string())?;
    chatos_memory_engine::list_contact_project_memories(
        contact.user_id.as_str(),
        contact.id.as_str(),
        project_id,
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
        .map(|item| item.project_id)
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
                .unwrap_or_else(|| if project_id == "0" { 1 } else { 0 }),
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

fn contact_to_dto(contact: ChatosContact) -> MemoryContactDto {
    MemoryContactDto {
        id: contact.id,
        user_id: contact.user_id,
        agent_id: contact.agent_id,
        agent_name_snapshot: contact.agent_name_snapshot,
        task_runner_enabled: contact.task_runner_enabled,
        task_runner_base_url: contact.task_runner_base_url,
        task_runner_username: contact.task_runner_username,
        task_runner_has_password: contact
            .task_runner_password
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        status: contact.status,
        created_at: contact.created_at,
        updated_at: contact.updated_at,
    }
}

fn project_to_dto(project: ChatosMemoryProject) -> MemoryProjectDto {
    MemoryProjectDto {
        id: project.id,
        user_id: project.user_id,
        project_id: project.project_id,
        name: project.name,
        root_path: project.root_path,
        description: project.description,
        status: project.status,
        is_virtual: project.is_virtual,
        created_at: project.created_at,
        updated_at: project.updated_at,
        archived_at: project.archived_at,
    }
}

fn project_agent_link_to_dto(link: ChatosProjectAgentLink) -> MemoryProjectAgentLinkDto {
    MemoryProjectAgentLinkDto {
        id: link.id,
        user_id: link.user_id,
        project_id: link.project_id,
        agent_id: link.agent_id,
        contact_id: link.contact_id,
        latest_session_id: link.latest_session_id,
        first_bound_at: link.first_bound_at,
        last_bound_at: link.last_bound_at,
        last_message_at: link.last_message_at,
        status: link.status,
        created_at: link.created_at,
        updated_at: link.updated_at,
    }
}

fn max_timestamp(left: &str, right: &str) -> String {
    if left >= right {
        left.to_string()
    } else {
        right.to_string()
    }
}

fn max_timestamp_opt(left: Option<&str>, right: Option<&str>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) => Some(max_timestamp(left, right)),
        (Some(left), None) => Some(left.to_string()),
        (None, Some(right)) => Some(right.to_string()),
        (None, None) => None,
    }
}
