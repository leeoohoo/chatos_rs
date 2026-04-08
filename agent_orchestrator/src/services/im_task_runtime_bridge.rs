use serde_json::json;
use tracing::warn;

use crate::models::session::Session;
use crate::services::im_service_client::{
    self, ConversationMessageDto, CreateConversationMessageRequestDto,
    PublishConversationEventRequestDto,
};
use crate::services::memory_server_client;
use crate::services::task_service_client::TaskRecordDto;

pub async fn publish_task_runtime_update(task: &TaskRecordDto) -> Result<(), String> {
    let Some(conversation_id) = resolve_task_conversation_id(task).await? else {
        return Ok(());
    };
    let owner_user_id = task.user_id.trim();
    if owner_user_id.is_empty() {
        return Ok(());
    }

    im_service_client::publish_internal_event(&PublishConversationEventRequestDto {
        owner_user_id: owner_user_id.to_string(),
        event_type: "im.task_execution.updated".to_string(),
        conversation_id,
        field_name: "task_execution".to_string(),
        payload: json!({
            "task_id": task.id,
            "status": task.status,
            "title": task.title,
            "session_id": task.session_id,
            "scope_key": task.scope_key,
            "project_id": task.project_id,
            "contact_agent_id": task.contact_agent_id,
            "updated_at": task.updated_at,
            "started_at": task.started_at,
            "finished_at": task.finished_at,
        }),
    })
    .await?;
    Ok(())
}

pub async fn publish_task_runtime_update_best_effort(task: &TaskRecordDto) {
    if let Err(err) = publish_task_runtime_update(task).await {
        warn!(
            "[IM-TASK-BRIDGE] publish task runtime update failed: task_id={} status={} error={}",
            task.id, task.status, err
        );
    }
}

pub async fn publish_task_notice_message(
    session_id: &str,
    event: &str,
    content: &str,
    task: Option<&TaskRecordDto>,
    legacy_message_id: Option<&str>,
) -> Result<Option<ConversationMessageDto>, String> {
    let Some(target) = resolve_session_im_target(session_id).await? else {
        return Ok(None);
    };
    let trimmed_content = content.trim();
    if trimmed_content.is_empty() {
        return Ok(None);
    }

    let message = im_service_client::create_conversation_message_internal(
        target.conversation_id.as_str(),
        &CreateConversationMessageRequestDto {
            sender_type: "contact".to_string(),
            sender_id: Some(target.contact_id),
            message_type: Some("text".to_string()),
            content: trimmed_content.to_string(),
            delivery_status: Some("sent".to_string()),
            client_message_id: None,
            reply_to_message_id: None,
            metadata: Some(json!({
                "task_execution": {
                    "event": event,
                    "legacy_session_id": session_id,
                    "legacy_message_id": legacy_message_id
                        .map(str::trim)
                        .filter(|value| !value.is_empty()),
                    "task_id": task.map(|item| item.id.clone()),
                    "task_title": task.map(|item| item.title.clone()),
                    "task_status": task.map(|item| item.status.clone()),
                    "scope_key": task.map(|item| item.scope_key.clone()),
                    "project_id": task.map(|item| item.project_id.clone()),
                    "contact_agent_id": task.map(|item| item.contact_agent_id.clone()),
                }
            })),
        },
    )
    .await?;

    Ok(Some(message))
}

pub async fn publish_task_notice_message_best_effort(
    session_id: &str,
    event: &str,
    content: &str,
    task: Option<&TaskRecordDto>,
    legacy_message_id: Option<&str>,
) {
    if let Err(err) =
        publish_task_notice_message(session_id, event, content, task, legacy_message_id).await
    {
        warn!(
            "[IM-TASK-BRIDGE] publish task notice message failed: session_id={} event={} task_id={} error={}",
            session_id,
            event,
            task.map(|item| item.id.as_str()).unwrap_or(""),
            err
        );
    }
}

async fn resolve_task_conversation_id(task: &TaskRecordDto) -> Result<Option<String>, String> {
    let Some(session_id) = task
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    Ok(resolve_session_im_target(session_id)
        .await?
        .map(|target| target.conversation_id))
}

struct SessionImTarget {
    conversation_id: String,
    contact_id: String,
}

async fn resolve_session_im_target(session_id: &str) -> Result<Option<SessionImTarget>, String> {
    let session = memory_server_client::get_session_by_id(session_id).await?;
    Ok(session.as_ref().and_then(session_im_target))
}

fn session_im_target(session: &Session) -> Option<SessionImTarget> {
    let metadata = session.metadata.as_ref()?;
    let im = metadata.get("im")?;
    let conversation_id = im
        .get("conversation_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let contact_id = im
        .get("contact_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    Some(SessionImTarget {
        conversation_id,
        contact_id,
    })
}
