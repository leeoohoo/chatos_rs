use crate::models::memory_mapping::{ChatosContact, ChatosMemoryProject, ChatosProjectAgentLink};
use crate::models::memory_mapping_types::{
    MemoryContactDto, MemoryProjectAgentLinkDto, MemoryProjectDto,
};

pub(super) fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn contact_to_dto(contact: ChatosContact) -> MemoryContactDto {
    MemoryContactDto {
        id: contact.id,
        user_id: contact.user_id,
        agent_id: contact.agent_id,
        agent_name_snapshot: contact.agent_name_snapshot,
        task_runner_enabled: contact.task_runner_enabled,
        task_runner_base_url: contact.task_runner_base_url,
        task_runner_agent_account_id: contact.task_runner_agent_account_id,
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

pub(super) fn project_to_dto(project: ChatosMemoryProject) -> MemoryProjectDto {
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

pub(super) fn project_agent_link_to_dto(link: ChatosProjectAgentLink) -> MemoryProjectAgentLinkDto {
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

pub(super) fn max_timestamp(left: &str, right: &str) -> String {
    if left >= right {
        left.to_string()
    } else {
        right.to_string()
    }
}

pub(super) fn max_timestamp_opt(left: Option<&str>, right: Option<&str>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) => Some(max_timestamp(left, right)),
        (Some(left), None) => Some(left.to_string()),
        (None, Some(right)) => Some(right.to_string()),
        (None, None) => None,
    }
}
