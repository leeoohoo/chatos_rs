use serde::Serialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::models::{now_rfc3339, AskUserPromptRecord, TaskRecord, TaskRunRecord, TaskStatus};

use super::support::status_label;
use super::*;

#[derive(Debug, Clone, Serialize)]
struct ChatosAskUserPromptCallbackPayload {
    event: String,
    task_id: String,
    run_id: Option<String>,
    status: String,
    task_title: String,
    task_status: String,
    project_id: String,
    source_session_id: Option<String>,
    source_turn_id: Option<String>,
    source_user_message_id: Option<String>,
    prompt: ChatosAskUserPromptPayload,
    callback_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChatosAskUserPromptPayload {
    prompt_id: String,
    kind: String,
    title: String,
    message: String,
    allow_cancel: bool,
    timeout_ms: u64,
    payload: Value,
    response: Option<AskUserResponseSubmission>,
    status: String,
    expires_at: Option<String>,
}

impl AskUserPromptService {
    pub(in crate::ask_user_prompt_service) async fn try_send_chatos_ask_user_prompt_required(
        &self,
        prompt: &AskUserPromptRecord,
    ) {
        self.try_send_chatos_ask_user_prompt_callback("ask_user_prompt.required", prompt)
            .await;
    }

    pub(in crate::ask_user_prompt_service) async fn try_send_chatos_ask_user_prompt_resolved(
        &self,
        prompt: &AskUserPromptRecord,
    ) {
        self.try_send_chatos_ask_user_prompt_callback("ask_user_prompt.resolved", prompt)
            .await;
    }

    async fn try_send_chatos_ask_user_prompt_callback(
        &self,
        event: &str,
        prompt: &AskUserPromptRecord,
    ) {
        let Some(config) = self.config.clone() else {
            return;
        };
        if config.chatos_callback_url.is_none() {
            return;
        }
        let Some((task, run)) = self.load_prompt_task_snapshot(prompt).await else {
            return;
        };
        if !has_chatos_source_context(&task) {
            return;
        }
        let payload =
            build_chatos_ask_user_prompt_callback_payload(event, &task, run.as_ref(), prompt);
        let prompt_id = payload.prompt.prompt_id.clone();
        let task_id = payload.task_id.clone();
        let run_id = payload.run_id.clone().unwrap_or_default();
        if let Err(err) = send_chatos_ask_user_prompt_callback(config, payload).await {
            warn!(
                task_id = task_id.as_str(),
                run_id = run_id.as_str(),
                prompt_id = prompt_id.as_str(),
                event,
                "failed to send ask user prompt callback to chatos: {}",
                err
            );
        } else {
            info!(
                task_id = task_id.as_str(),
                run_id = run_id.as_str(),
                prompt_id = prompt_id.as_str(),
                event,
                "sent ask user prompt callback to chatos"
            );
        }
    }

    async fn load_prompt_task_snapshot(
        &self,
        prompt: &AskUserPromptRecord,
    ) -> Option<(TaskRecord, Option<TaskRunRecord>)> {
        let run = match prompt.run_id.as_deref() {
            Some(run_id) => match self.store.get_run(run_id).await {
                Ok(run) => run,
                Err(err) => {
                    warn!(
                        prompt_id = prompt.id.as_str(),
                        run_id, "failed to load ask user prompt run snapshot for callback: {}", err
                    );
                    return None;
                }
            },
            None => None,
        };
        let task_id = prompt
            .task_id
            .clone()
            .or_else(|| run.as_ref().map(|run| run.task_id.clone()))?;
        let task = match self.store.get_task(task_id.as_str()).await {
            Ok(task) => task,
            Err(err) => {
                warn!(
                    prompt_id = prompt.id.as_str(),
                    task_id = task_id.as_str(),
                    "failed to load ask user prompt task snapshot for callback: {}",
                    err
                );
                return None;
            }
        }?;
        Some((task, run))
    }
}

fn build_chatos_ask_user_prompt_callback_payload(
    event: &str,
    task: &TaskRecord,
    run: Option<&TaskRunRecord>,
    prompt: &AskUserPromptRecord,
) -> ChatosAskUserPromptCallbackPayload {
    ChatosAskUserPromptCallbackPayload {
        event: event.to_string(),
        task_id: task.id.clone(),
        run_id: prompt
            .run_id
            .clone()
            .or_else(|| run.map(|item| item.id.clone())),
        status: task_status_label(task.status).to_string(),
        task_title: task.title.clone(),
        task_status: task_status_label(task.status).to_string(),
        project_id: task.project_id.clone(),
        source_session_id: task.source_session_id.clone(),
        source_turn_id: task.source_turn_id.clone(),
        source_user_message_id: task.source_user_message_id.clone(),
        prompt: ChatosAskUserPromptPayload {
            prompt_id: prompt.id.clone(),
            kind: prompt.kind.clone(),
            title: prompt.title.clone(),
            message: prompt.message.clone(),
            allow_cancel: prompt.allow_cancel,
            timeout_ms: prompt.timeout_ms,
            payload: prompt.payload.clone(),
            response: prompt.response.clone(),
            status: status_label(prompt.status).to_string(),
            expires_at: prompt.expires_at.clone(),
        },
        callback_at: now_rfc3339(),
    }
}

async fn send_chatos_ask_user_prompt_callback(
    config: AppConfig,
    payload: ChatosAskUserPromptCallbackPayload,
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

fn has_chatos_source_context(task: &TaskRecord) -> bool {
    has_non_empty_text(task.source_session_id.as_deref())
        && has_non_empty_text(task.source_user_message_id.as_deref())
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Draft => "draft",
        TaskStatus::Ready => "ready",
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Succeeded => "succeeded",
        TaskStatus::Failed => "failed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Archived => "archived",
    }
}
