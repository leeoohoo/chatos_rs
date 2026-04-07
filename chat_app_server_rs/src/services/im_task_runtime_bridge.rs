use serde_json::json;
use tracing::warn;

use crate::models::session::Session;
use crate::services::im_service_client::{self, PublishConversationEventRequestDto};
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

async fn resolve_task_conversation_id(task: &TaskRecordDto) -> Result<Option<String>, String> {
    let Some(session_id) = task
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let session = memory_server_client::get_session_by_id(session_id).await?;
    Ok(session
        .as_ref()
        .and_then(session_im_conversation_id)
        .map(ToOwned::to_owned))
}

fn session_im_conversation_id(session: &Session) -> Option<&str> {
    session
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("im"))
        .and_then(|value| value.get("conversation_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
