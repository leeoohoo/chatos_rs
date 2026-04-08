use crate::core::chat_runtime::ChatRuntimeMetadata;
use crate::services::contact_agent_model::resolve_effective_contact_agent_model_config_id;
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::{TaskHandoffPayload, TaskRecord, TaskResultBrief};
use crate::services::{memory_server_client, task_service_client};

#[derive(Debug, Clone)]
pub struct TaskScopeContext {
    pub user_id: String,
    pub contact_id: Option<String>,
    pub contact_agent_id: String,
    pub project_id: String,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub model_config_id: Option<String>,
}

pub async fn resolve_task_scope_context(session_id: &str) -> Result<TaskScopeContext, String> {
    let session_id =
        trimmed_non_empty(session_id).ok_or_else(|| "session_id is required".to_string())?;
    let session = memory_server_client::get_session_by_id(session_id)
        .await?
        .ok_or_else(|| "session not found".to_string())?;
    let metadata = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    let contact_id = metadata.contact_id.clone();
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
            user_id.as_str(),
            contact_id.as_deref(),
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
    let project_root = if let Some(root) = metadata.project_root.clone() {
        Some(root)
    } else if project_id != "0" {
        crate::repositories::projects::get_project_by_id(project_id.as_str())
            .await?
            .and_then(|project| {
                trimmed_non_empty(project.root_path.as_str()).map(ToOwned::to_owned)
            })
            .or_else(|| metadata.workspace_root.clone())
    } else {
        metadata.workspace_root.clone()
    };
    let remote_connection_id = metadata.remote_connection_id.clone();
    let model_config_id =
        resolve_effective_contact_agent_model_config_id(contact_agent_id.as_str()).await?;

    Ok(TaskScopeContext {
        user_id,
        contact_id,
        contact_agent_id,
        project_id,
        project_root,
        remote_connection_id,
        model_config_id,
    })
}

async fn resolve_contact_agent_id_from_contact_id(
    user_id: &str,
    contact_id: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(contact_id) = contact_id.and_then(trimmed_non_empty) else {
        return Ok(None);
    };

    Ok(
        memory_server_client::resolve_memory_contact(Some(user_id), Some(contact_id), None)
            .await?
            .and_then(|contact| {
                trimmed_non_empty(contact.agent_id.as_str()).map(ToOwned::to_owned)
            }),
    )
}

pub(super) fn map_remote_result_brief(
    brief: task_service_client::TaskResultBriefDto,
) -> TaskResultBrief {
    TaskResultBrief {
        task_id: brief.task_id,
        task_status: brief.task_status,
        result_summary: brief.result_summary,
        result_format: brief.result_format,
        result_message_id: brief.result_message_id,
        source_session_id: brief.source_session_id,
        source_turn_id: brief.source_turn_id,
        finished_at: brief.finished_at,
        updated_at: brief.updated_at,
    }
}

pub(super) fn map_remote_handoff_payload(
    handoff: task_service_client::TaskHandoffPayloadDto,
) -> TaskHandoffPayload {
    TaskHandoffPayload {
        task_id: handoff.task_id,
        task_plan_id: handoff.task_plan_id,
        handoff_kind: handoff.handoff_kind,
        summary: handoff.summary,
        result_summary: handoff.result_summary,
        key_changes: handoff.key_changes,
        changed_files: handoff.changed_files,
        executed_commands: handoff.executed_commands,
        verification_suggestions: handoff.verification_suggestions,
        open_risks: handoff.open_risks,
        artifact_refs: handoff.artifact_refs,
        checkpoint_message_ids: handoff.checkpoint_message_ids,
        result_brief_id: handoff.result_brief_id,
        generated_at: handoff.generated_at,
    }
}

pub(super) fn map_remote_task_to_record(
    task: task_service_client::TaskRecordDto,
    task_result_brief: Option<TaskResultBrief>,
) -> TaskRecord {
    TaskRecord {
        id: task.id,
        task_plan_id: task.task_plan_id,
        task_ref: task.task_ref,
        task_kind: task.task_kind,
        depends_on_task_ids: task.depends_on_task_ids,
        verification_of_task_ids: task.verification_of_task_ids,
        acceptance_criteria: task.acceptance_criteria,
        blocked_reason: task.blocked_reason,
        session_id: task.session_id.unwrap_or_default(),
        conversation_turn_id: task.conversation_turn_id.unwrap_or_default(),
        project_root: task.project_root,
        remote_connection_id: task.remote_connection_id,
        title: task.title,
        details: task.content,
        priority: normalize_remote_priority(task.priority.as_str()),
        status: normalize_remote_status(task.status.as_str()),
        tags: Vec::new(),
        due_at: None,
        planned_builtin_mcp_ids: task.planned_builtin_mcp_ids,
        planned_context_assets: task.planned_context_assets,
        execution_result_contract: task.execution_result_contract,
        planning_snapshot: task.planning_snapshot,
        result_summary: task.result_summary,
        last_error: task.last_error,
        confirmed_at: task.confirmed_at,
        started_at: task.started_at,
        finished_at: task.finished_at,
        created_at: task.created_at,
        updated_at: task.updated_at,
        task_result_brief,
        handoff_payload: task.handoff_payload.map(map_remote_handoff_payload),
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
        "paused" => "paused".to_string(),
        "blocked" => "blocked".to_string(),
        "completed" => "completed".to_string(),
        "failed" => "failed".to_string(),
        "cancelled" => "cancelled".to_string(),
        "skipped" => "skipped".to_string(),
        _ => "pending_confirm".to_string(),
    }
}

pub(super) fn map_legacy_status_to_remote(value: Option<String>) -> Option<String> {
    value.map(|status| match status.trim().to_ascii_lowercase().as_str() {
        "pending_confirm" => "pending_confirm".to_string(),
        "pending_execute" => "pending_execute".to_string(),
        "running" => "running".to_string(),
        "paused" => "paused".to_string(),
        "blocked" => "blocked".to_string(),
        "completed" => "completed".to_string(),
        "failed" => "failed".to_string(),
        "cancelled" => "cancelled".to_string(),
        "skipped" => "skipped".to_string(),
        _ => "pending_confirm".to_string(),
    })
}
