use crate::models::memory_mapping_types::{
    CreateMemoryContactRequestDto, CreateMemoryContactResponseDto, MemoryContactDto,
    UpdateContactTaskRunnerConfigRequestDto,
};
use crate::repositories::chatos_memory_mappings as mappings_repo;

use super::support::{contact_to_dto, normalize_non_empty};

#[derive(Debug, Clone)]
pub struct ContactTaskRunnerRuntimeConfig {
    pub contact_id: String,
    pub base_url: String,
    pub username: String,
    pub password: String,
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
