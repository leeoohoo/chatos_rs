use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::TaskRecord;
use crate::services::task_service_client;
use tracing::warn;

use super::remote_support::{
    map_remote_result_brief, map_remote_task_to_record, resolve_task_scope_context,
};

fn is_terminal_task_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "failed" | "cancelled" | "skipped"
    )
}

pub async fn list_tasks_for_context(
    session_id: &str,
    conversation_turn_id: Option<&str>,
    include_done: bool,
    limit: usize,
) -> Result<Vec<TaskRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let conversation_turn_id = conversation_turn_id
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let scope = resolve_task_scope_context(session_id.as_str()).await?;

    let items = task_service_client::list_tasks(
        Some(scope.user_id.as_str()),
        Some(scope.contact_agent_id.as_str()),
        Some(scope.project_id.as_str()),
        Some(session_id.as_str()),
        conversation_turn_id.as_deref(),
        None,
        Some(limit.clamp(1, 200) as i64),
        0,
    )
    .await?;

    let mut enriched = Vec::with_capacity(items.len());
    for task in items {
        let task_id = task.id.clone();
        let task_result_brief =
            match task_service_client::get_task_result_brief(task_id.as_str()).await {
                Ok(item) => item.map(map_remote_result_brief),
                Err(err) => {
                    warn!(
                        "load task result brief failed: task_id={} detail={}",
                        task_id, err
                    );
                    None
                }
            };
        enriched.push(map_remote_task_to_record(task, task_result_brief));
    }

    let mut out = Vec::new();
    for record in enriched {
        if !include_done && is_terminal_task_status(record.status.as_str()) {
            continue;
        }
        out.push(record);
    }
    Ok(out)
}
