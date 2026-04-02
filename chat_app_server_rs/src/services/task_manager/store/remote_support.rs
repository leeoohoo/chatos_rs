use crate::core::chat_runtime::ChatRuntimeMetadata;
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::TaskRecord;
use crate::services::{memory_server_client, task_service_client};
use tracing::warn;

#[derive(Debug, Clone)]
pub(super) struct TaskScopeContext {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub model_config_id: Option<String>,
}

pub(super) async fn resolve_task_scope_context(
    session_id: &str,
) -> Result<TaskScopeContext, String> {
    let session_id =
        trimmed_non_empty(session_id).ok_or_else(|| "session_id is required".to_string())?;
    let session = memory_server_client::get_session_by_id(session_id)
        .await?
        .ok_or_else(|| "session not found".to_string())?;
    let metadata = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    let user_id = session
        .user_id
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "session user_id is missing".to_string())?;
    let contact_agent_id = session
        .selected_agent_id
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(ToOwned::to_owned)
        .or(metadata.contact_agent_id)
        .or(resolve_contact_agent_id_from_contact_id(
            session_id,
            user_id.as_str(),
            metadata.contact_id.as_deref(),
        )
        .await?)
        .ok_or_else(|| "contact_agent_id is required for task operations".to_string())?;
    let project_id = session
        .project_id
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(ToOwned::to_owned)
        .or(metadata.project_id)
        .unwrap_or_else(|| "0".to_string());
    let model_config_id =
        match memory_server_client::get_memory_agent_runtime_context(contact_agent_id.as_str())
            .await
        {
            Ok(Some(context)) => context
                .model_config_id
                .as_deref()
                .and_then(trimmed_non_empty)
                .map(ToOwned::to_owned),
            _ => None,
        };

    Ok(TaskScopeContext {
        user_id,
        contact_agent_id,
        project_id,
        model_config_id,
    })
}

async fn resolve_contact_agent_id_from_contact_id(
    session_id: &str,
    user_id: &str,
    contact_id: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(contact_id) = contact_id.and_then(trimmed_non_empty) else {
        return Ok(None);
    };

    let contacts = memory_server_client::list_memory_contacts(Some(user_id), Some(500), 0).await?;
    let resolved = contacts
        .into_iter()
        .find(|contact| contact.id == contact_id)
        .map(|contact| contact.agent_id)
        .and_then(|value| trimmed_non_empty(value.as_str()).map(ToOwned::to_owned));

    if let Some(contact_agent_id) = resolved.as_ref() {
        warn!(
            "resolved contact_agent_id from contact_id: session_id={} contact_id={} contact_agent_id={}",
            session_id,
            contact_id,
            contact_agent_id
        );
    }

    Ok(resolved)
}

pub(super) fn map_remote_task_to_record(task: task_service_client::TaskRecordDto) -> TaskRecord {
    TaskRecord {
        id: task.id,
        session_id: task.session_id.unwrap_or_default(),
        conversation_turn_id: task.conversation_turn_id.unwrap_or_default(),
        title: task.title,
        details: task.content,
        priority: normalize_remote_priority(task.priority.as_str()),
        status: normalize_remote_status(task.status.as_str()),
        tags: Vec::new(),
        due_at: None,
        created_at: task.created_at,
        updated_at: task.updated_at,
    }
}

pub(super) fn normalize_remote_priority(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => "high".to_string(),
        "low" => "low".to_string(),
        _ => "medium".to_string(),
    }
}

pub(super) fn normalize_remote_status(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "pending_confirm" => "pending_confirm".to_string(),
        "pending_execute" => "pending_execute".to_string(),
        "running" => "running".to_string(),
        "completed" => "completed".to_string(),
        "failed" => "failed".to_string(),
        "cancelled" => "cancelled".to_string(),
        _ => "pending_confirm".to_string(),
    }
}

pub(super) fn map_legacy_status_to_remote(value: Option<String>) -> Option<String> {
    value.map(|status| match status.trim().to_ascii_lowercase().as_str() {
        "pending_confirm" => "pending_confirm".to_string(),
        "pending_execute" => "pending_execute".to_string(),
        "running" => "running".to_string(),
        "completed" => "completed".to_string(),
        "failed" => "failed".to_string(),
        "cancelled" => "cancelled".to_string(),
        _ => "pending_confirm".to_string(),
    })
}
