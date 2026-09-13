// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::memory_mapping_types::{
    CreateMemoryContactRequestDto, CreateMemoryContactResponseDto, MemoryContactDto,
};
use crate::repositories::chatos_memory_mappings as mappings_repo;

use super::support::contact_to_dto;

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
