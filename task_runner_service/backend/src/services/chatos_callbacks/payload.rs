use super::*;

pub(super) async fn load_task_snapshot_for_callback(
    store: &AppStore,
    task_id: &str,
) -> Result<Option<TaskRecord>, String> {
    let Some(mut task) = store.get_task(task_id).await? else {
        return Ok(None);
    };
    task.prerequisite_task_ids = store
        .list_task_prerequisites(task_id)
        .await?
        .into_iter()
        .map(|item| item.prerequisite_task_id)
        .collect();
    Ok(Some(task))
}

pub(super) fn build_chatos_task_callback_payload(
    event: &str,
    task: &TaskRecord,
    run: Option<&TaskRunRecord>,
    error_message: Option<String>,
) -> Option<ChatosTaskCallbackPayload> {
    if task
        .source_user_message_id
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return None;
    }
    Some(ChatosTaskCallbackPayload {
        event: event.to_string(),
        task_id: task.id.clone(),
        run_id: run.map(|item| item.id.clone()),
        status: task.status.status_string().to_string(),
        task_title: task.title.clone(),
        result_summary: normalize_optional_callback_text(
            run.and_then(|item| item.result_summary.clone())
                .or_else(|| task.result_summary.clone()),
        ),
        error_message: normalize_optional_callback_text(
            error_message.or_else(|| run.and_then(|item| item.error_message.clone())),
        ),
        report_content: run.and_then(extract_report_content),
        process_log: None,
        source_session_id: task.source_session_id.clone(),
        source_turn_id: task.source_turn_id.clone(),
        source_user_message_id: task.source_user_message_id.clone(),
        parent_task_id: task.parent_task_id.clone(),
        source_run_id: task.source_run_id.clone(),
        prerequisite_task_ids: task.prerequisite_task_ids.clone(),
        cancel_reason: task.task_tool_state.cancel_reason.clone(),
        cancelled_at: task.task_tool_state.cancelled_at.clone(),
        cancelled_by_user_id: task.task_tool_state.cancelled_by_user_id.clone(),
        cancelled_by_username: task.task_tool_state.cancelled_by_username.clone(),
        cancelled_by_display_name: task.task_tool_state.cancelled_by_display_name.clone(),
        replacement_task_ids: task.task_tool_state.replacement_task_ids.clone(),
        cancelled_because_task_id: task.task_tool_state.cancelled_because_task_id.clone(),
        cascade_root_task_id: task.task_tool_state.cascade_root_task_id.clone(),
        schedule_mode: task.schedule.mode.mode_key().to_string(),
        callback_at: now_rfc3339(),
    })
}

fn normalize_optional_callback_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
