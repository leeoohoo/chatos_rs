use serde::Serialize;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::{now_rfc3339, TaskRecord, TaskRunRecord, TaskRunStatus};
use crate::store::AppStore;

use super::prerequisite_context::extract_report_content;
use super::{RunService, TaskScheduleModeExt, TaskStatusExt};

#[derive(Debug, Clone, Serialize)]
struct ChatosTaskCallbackPayload {
    event: String,
    task_id: String,
    run_id: Option<String>,
    status: String,
    task_title: String,
    result_summary: Option<String>,
    error_message: Option<String>,
    report_content: Option<String>,
    process_log: Option<String>,
    source_session_id: Option<String>,
    source_turn_id: Option<String>,
    source_user_message_id: Option<String>,
    parent_task_id: Option<String>,
    source_run_id: Option<String>,
    prerequisite_task_ids: Vec<String>,
    schedule_mode: String,
    callback_at: String,
}

impl RunService {
    pub(super) async fn try_send_terminal_callback(&self, task_id: &str, run: &TaskRunRecord) {
        let event = match run.status {
            TaskRunStatus::Succeeded => "task.completed",
            TaskRunStatus::Failed
            | TaskRunStatus::Cancelled
            | TaskRunStatus::Blocked
            | TaskRunStatus::Queued
            | TaskRunStatus::Running => return,
        };
        self.try_send_task_callback(event, task_id, Some(run)).await;
    }

    pub(super) async fn try_send_task_callback(
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
        if let Err(err) = send_chatos_task_callback(self.config.clone(), payload).await {
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

async fn load_task_snapshot_for_callback(
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

fn build_chatos_task_callback_payload(
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
        schedule_mode: task.schedule.mode.mode_key().to_string(),
        callback_at: now_rfc3339(),
    })
}

async fn send_chatos_task_callback(
    config: AppConfig,
    payload: ChatosTaskCallbackPayload,
) -> Result<(), String> {
    let Some(url) = config.chatos_callback_url.clone() else {
        return Err("TASK_RUNNER_CHATOS_CALLBACK_URL not configured".to_string());
    };
    let client = reqwest::Client::builder()
        .timeout(config.callback_timeout)
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.post(url).json(&payload);
    if let Some(secret) = config.chatos_callback_secret.clone() {
        request = request.header("X-Task-Runner-Callback-Secret", secret);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    Err(format!("callback request failed: {status} {body}"))
}

fn normalize_optional_callback_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
