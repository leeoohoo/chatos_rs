use crate::models::memory_mapping_types::{
    MemoryProjectAgentLinkDto, MemoryProjectContactDto, SyncProjectAgentLinkRequestDto,
};
use crate::repositories::chatos_memory_mappings as mappings_repo;
use crate::repositories::projects;

use super::support::{max_timestamp, project_agent_link_to_dto};

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
