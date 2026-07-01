// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

use super::payload::{build_chatos_task_callback_payload, load_task_snapshot_for_callback};

impl RunService {
    pub(in crate::services) async fn try_send_terminal_callback(
        &self,
        task_id: &str,
        run: &TaskRunRecord,
    ) {
        let Some(event) = terminal_callback_event_for_status(run.status) else {
            return;
        };
        self.try_send_task_callback(event, task_id, Some(run)).await;
    }

    pub(in crate::services) async fn try_send_task_callback(
        &self,
        event: &str,
        task_id: &str,
        run: Option<&TaskRunRecord>,
    ) {
        let task = match load_task_snapshot_for_callback(&self.store, task_id).await {
            Ok(Some(task)) => task,
            Ok(None) => return,
            Err(err) => {
                warn!(
                    "failed to load callback task snapshot for task {} and event {}: {}",
                    task_id, event, err
                );
                return;
            }
        };
        let Some(payload) = build_chatos_task_callback_payload(event, &task, run, None) else {
            if task.source_session_id.is_some()
                || task.source_turn_id.is_some()
                || task.source_run_id.is_some()
            {
                warn!(
                    task_id = task.id.as_str(),
                    task_title = task.title.as_str(),
                    event,
                    source_session_id = task.source_session_id.as_deref().unwrap_or_default(),
                    source_turn_id = task.source_turn_id.as_deref().unwrap_or_default(),
                    source_user_message_id =
                        task.source_user_message_id.as_deref().unwrap_or_default(),
                    "skip task callback because source_user_message_id is missing"
                );
            }
            return;
        };
        let payload_task_id = payload.task_id.clone();
        let payload_run_id = payload.run_id.clone().unwrap_or_default();
        let payload_user_message_id = payload.source_user_message_id.clone().unwrap_or_default();
        let payload_event = payload.event.clone();
        if let Err(err) =
            super::delivery::send_chatos_task_callback(self.config.clone(), payload).await
        {
            warn!(
                "failed to send task callback for task {} and event {}: {}",
                task_id, event, err
            );
        } else {
            info!(
                task_id = payload_task_id.as_str(),
                run_id = payload_run_id.as_str(),
                event = payload_event.as_str(),
                source_user_message_id = payload_user_message_id.as_str(),
                "sent task callback to chatos"
            );
        }
    }
}

impl TaskService {
    pub(in crate::services) async fn try_send_task_callback(
        &self,
        event: &str,
        task_id: &str,
        run: Option<&TaskRunRecord>,
    ) {
        send_task_callback_with_store(&self.config, &self.store, event, task_id, run).await;
    }
}

async fn send_task_callback_with_store(
    config: &AppConfig,
    store: &AppStore,
    event: &str,
    task_id: &str,
    run: Option<&TaskRunRecord>,
) {
    let task = match load_task_snapshot_for_callback(store, task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => return,
        Err(err) => {
            warn!(
                "failed to load callback task snapshot for task {} and event {}: {}",
                task_id, event, err
            );
            return;
        }
    };
    let Some(payload) = build_chatos_task_callback_payload(event, &task, run, None) else {
        if task.source_session_id.is_some()
            || task.source_turn_id.is_some()
            || task.source_run_id.is_some()
        {
            warn!(
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                event,
                source_session_id = task.source_session_id.as_deref().unwrap_or_default(),
                source_turn_id = task.source_turn_id.as_deref().unwrap_or_default(),
                source_user_message_id = task.source_user_message_id.as_deref().unwrap_or_default(),
                "skip task callback because source_user_message_id is missing"
            );
        }
        return;
    };
    let payload_task_id = payload.task_id.clone();
    let payload_run_id = payload.run_id.clone().unwrap_or_default();
    let payload_user_message_id = payload.source_user_message_id.clone().unwrap_or_default();
    let payload_event = payload.event.clone();
    if let Err(err) = super::delivery::send_chatos_task_callback(config.clone(), payload).await {
        warn!(
            "failed to send task callback for task {} and event {}: {}",
            task_id, event, err
        );
    } else {
        info!(
            task_id = payload_task_id.as_str(),
            run_id = payload_run_id.as_str(),
            event = payload_event.as_str(),
            source_user_message_id = payload_user_message_id.as_str(),
            "sent task callback to chatos"
        );
    }
}

fn terminal_callback_event_for_status(status: TaskRunStatus) -> Option<&'static str> {
    match status {
        TaskRunStatus::Succeeded => Some("task.completed"),
        TaskRunStatus::Failed => Some("task.failed"),
        TaskRunStatus::Cancelled => Some("task.cancelled"),
        TaskRunStatus::Blocked => Some("task.blocked"),
        TaskRunStatus::Queued | TaskRunStatus::Running => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{terminal_callback_event_for_status, TaskRunStatus};

    #[test]
    fn terminal_callback_event_for_status_covers_all_terminal_states() {
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Succeeded),
            Some("task.completed")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Failed),
            Some("task.failed")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Cancelled),
            Some("task.cancelled")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Blocked),
            Some("task.blocked")
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Queued),
            None
        );
        assert_eq!(
            terminal_callback_event_for_status(TaskRunStatus::Running),
            None
        );
    }
}
