use crate::services::task_manager::normalizer::{normalize_task_drafts, trimmed_non_empty};
use crate::services::task_manager::types::{TaskDraft, TaskRecord};
use crate::services::task_service_client::{self, ConfirmTaskRequestDto, CreateTaskRequestDto};

use super::remote_support::{map_remote_task_to_record, resolve_task_scope_context};

fn is_model_not_configured_error(err: &str) -> bool {
    err.contains("当前联系人未配置执行模型")
}

pub async fn create_tasks_for_turn(
    session_id: &str,
    conversation_turn_id: &str,
    draft_tasks: Vec<TaskDraft>,
) -> Result<Vec<TaskRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(conversation_turn_id)
        .ok_or_else(|| "conversation_turn_id is required".to_string())?
        .to_string();
    let draft_tasks = normalize_task_drafts(draft_tasks)?;
    if draft_tasks.is_empty() {
        return Ok(Vec::new());
    }

    let scope = resolve_task_scope_context(session_id.as_str()).await?;
    let mut out = Vec::with_capacity(draft_tasks.len());
    for draft in draft_tasks {
        let created = task_service_client::create_task(&CreateTaskRequestDto {
            user_id: Some(scope.user_id.clone()),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            session_id: Some(session_id.clone()),
            conversation_turn_id: Some(conversation_turn_id.clone()),
            source_message_id: None,
            model_config_id: scope.model_config_id.clone(),
            title: draft.title.clone(),
            content: if draft.details.trim().is_empty() {
                draft.title.clone()
            } else {
                draft.details.clone()
            },
            priority: Some(draft.priority.clone()),
            confirm_note: None,
            execution_note: None,
        })
        .await?;

        let final_task = match task_service_client::confirm_task(
            created.id.as_str(),
            &ConfirmTaskRequestDto {
                user_id: Some(scope.user_id.clone()),
                note: None,
            },
        )
        .await
        {
            Ok(Some(task)) => task,
            Ok(None) => created,
            Err(err) if is_model_not_configured_error(err.as_str()) => created,
            Err(err) => return Err(err),
        };
        out.push(map_remote_task_to_record(final_task));
    }

    Ok(out)
}
