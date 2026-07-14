// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::Config;
use crate::models::memory_mapping::ChatosContact;
use crate::models::memory_mapping_types::{
    CreateMemoryContactRequestDto, CreateMemoryContactResponseDto, MemoryContactDto,
    UpdateContactTaskRunnerConfigRequestDto,
};
use crate::repositories::chatos_memory_mappings as mappings_repo;
use crate::services::chatos_agents;

use super::support::{contact_to_dto, normalize_non_empty};

#[derive(Debug, Clone)]
pub struct ContactTaskRunnerRuntimeConfig {
    pub contact_id: String,
    pub base_url: String,
    pub agent_account_id: Option<String>,
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
    let Some(stored_base_url) = normalize_non_empty(contact.task_runner_base_url.as_deref()) else {
        return Ok(None);
    };
    let base_url = resolve_runtime_task_runner_base_url(
        stored_base_url.as_str(),
        default_task_runner_base_url().as_deref(),
    );
    let agent_account_id = normalize_non_empty(contact.task_runner_agent_account_id.as_deref());
    if agent_account_id.is_none() {
        return Ok(None);
    }

    Ok(Some(ContactTaskRunnerRuntimeConfig {
        contact_id: contact.id,
        base_url,
        agent_account_id,
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
    let contact = auto_bind_contact_task_runner(contact, created).await?;
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
    let Some(existing) = mappings_repo::get_contact_by_id(contact_id).await? else {
        return Ok(None);
    };
    let resolved_base_url = normalize_non_empty(payload.base_url.as_deref())
        .or_else(|| normalize_non_empty(existing.task_runner_base_url.as_deref()))
        .or_else(default_task_runner_base_url);
    let contact = mappings_repo::update_contact_task_runner_config(
        contact_id,
        mappings_repo::UpdateContactTaskRunnerConfigInput {
            enabled: payload.enabled,
            base_url: resolved_base_url,
            agent_account_id: payload.task_runner_agent_account_id.clone(),
            username: payload.username.clone(),
            password: payload.password.clone(),
            clear_password: payload.clear_password.unwrap_or(false),
        },
    )
    .await?;
    Ok(contact.map(contact_to_dto))
}

async fn auto_bind_contact_task_runner(
    contact: ChatosContact,
    created: bool,
) -> Result<ChatosContact, String> {
    let Some(agent) = chatos_agents::get_agent(contact.agent_id.as_str()).await? else {
        return Ok(contact);
    };
    let Some(agent_account_id) = normalize_non_empty(agent.task_runner_agent_account_id.as_deref())
    else {
        return Ok(contact);
    };
    let base_url = normalize_non_empty(Some(Config::get().task_runner_base_url.as_str()))
        .ok_or_else(|| "task_runner_base_url is empty".to_string())?;
    if !should_auto_bind_contact_task_runner(
        &contact,
        created,
        agent_account_id.as_str(),
        base_url.as_str(),
    ) {
        return Ok(contact);
    }
    let updated = mappings_repo::update_contact_task_runner_config(
        contact.id.as_str(),
        mappings_repo::UpdateContactTaskRunnerConfigInput {
            enabled: true,
            base_url: Some(base_url),
            agent_account_id: Some(agent_account_id),
            username: None,
            password: None,
            clear_password: true,
        },
    )
    .await?;
    Ok(updated.unwrap_or(contact))
}

fn default_task_runner_base_url() -> Option<String> {
    normalize_non_empty(Some(Config::get().task_runner_base_url.as_str()))
}

fn resolve_runtime_task_runner_base_url(stored: &str, configured: Option<&str>) -> String {
    let stored = stored.trim().trim_end_matches('/');
    let configured = configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/'));
    if is_environment_scoped_task_runner_url(stored) {
        if let Some(configured) = configured {
            return configured.to_string();
        }
    }
    stored.to_string()
}

fn is_environment_scoped_task_runner_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("task-runner-backend")
            || host.eq_ignore_ascii_case("localhost")
            || host == "127.0.0.1"
            || host == "::1"
    })
}

fn should_auto_bind_contact_task_runner(
    contact: &ChatosContact,
    created: bool,
    agent_account_id: &str,
    base_url: &str,
) -> bool {
    if created {
        return true;
    }
    let existing_agent_account_id =
        normalize_non_empty(contact.task_runner_agent_account_id.as_deref());
    let existing_base_url = normalize_non_empty(contact.task_runner_base_url.as_deref());
    let has_legacy_credentials = normalize_non_empty(contact.task_runner_username.as_deref())
        .is_some()
        || normalize_non_empty(contact.task_runner_password.as_deref()).is_some();

    match existing_agent_account_id.as_deref() {
        Some(existing) if existing == agent_account_id => {
            existing_base_url.as_deref() != Some(base_url) || has_legacy_credentials
        }
        Some(_) => false,
        None => !has_legacy_credentials,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_runtime_task_runner_base_url;

    #[test]
    fn runtime_uses_current_environment_url_for_stale_docker_alias() {
        assert_eq!(
            resolve_runtime_task_runner_base_url(
                "http://task-runner-backend:39090",
                Some("http://127.0.0.1:39090"),
            ),
            "http://127.0.0.1:39090"
        );
    }

    #[test]
    fn runtime_preserves_explicit_external_task_runner_url() {
        assert_eq!(
            resolve_runtime_task_runner_base_url(
                "https://tasks.example.com",
                Some("http://127.0.0.1:39090"),
            ),
            "https://tasks.example.com"
        );
    }
}
