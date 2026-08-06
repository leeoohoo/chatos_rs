// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tracing::{info, warn};

use super::support::{
    redacted_prompt_payload, redacted_prompt_response, secret_field_keys, status_label,
};
use super::*;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::models::{now_rfc3339, AskUserPromptRecord, TaskRecord, TaskRunRecord, TaskStatus};

const ASK_USER_CALLBACK_RETRY_DELAYS_MS: [u64; 3] = [0, 250, 750];

#[derive(Debug, Clone, Serialize)]
struct ChatosAskUserPromptCallbackPayload {
    event: String,
    task_id: String,
    owner_user_id: Option<String>,
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
    let secret_keys = secret_field_keys(&prompt.payload);
    ChatosAskUserPromptCallbackPayload {
        event: event.to_string(),
        task_id: task.id.clone(),
        owner_user_id: task.owner_user_id.clone(),
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
            payload: redacted_prompt_payload(prompt.payload.clone()),
            response: redacted_prompt_response(prompt.response.clone(), &secret_keys),
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
    let url = config.chatos_callback_url.clone();
    let secret = config
        .chatos_internal_api_secret
        .as_deref()
        .ok_or_else(|| "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET not configured".to_string())?;
    let client = config.chatos_callback_http_client.clone();
    let mut last_error: Option<String> = None;
    for (attempt_index, delay_ms) in ASK_USER_CALLBACK_RETRY_DELAYS_MS.into_iter().enumerate() {
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        let mut request = client.post(url.clone()).json(&payload);
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            "task-runner",
            "chatos-backend",
            "task-runner.callback",
            60,
        )?;
        request = request
            .header("X-Chatos-Internal-Caller", "task-runner")
            .header("X-Chatos-Internal-Token", token);
        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                last_error = Some(err.to_string());
                if attempt_index + 1 < ASK_USER_CALLBACK_RETRY_DELAYS_MS.len() {
                    warn!(
                        attempt = attempt_index + 1,
                        max_attempts = ASK_USER_CALLBACK_RETRY_DELAYS_MS.len(),
                        "ask user prompt callback delivery failed; retrying: {}",
                        err
                    );
                }
                continue;
            }
        };
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        let error = format!("callback request failed: {status} {body}");
        if !ask_user_callback_status_is_retryable(status) {
            return Err(error);
        }
        last_error = Some(error.clone());
        if attempt_index + 1 < ASK_USER_CALLBACK_RETRY_DELAYS_MS.len() {
            warn!(
                attempt = attempt_index + 1,
                max_attempts = ASK_USER_CALLBACK_RETRY_DELAYS_MS.len(),
                "ask user prompt callback delivery failed; retrying: {}",
                error
            );
        }
    }
    Err(last_error.unwrap_or_else(|| "callback request failed".to_string()))
}

fn ask_user_callback_status_is_retryable(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
        || matches!(
            status,
            reqwest::StatusCode::REQUEST_TIMEOUT | reqwest::StatusCode::TOO_MANY_REQUESTS
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_user_callback_retries_only_transient_http_failures() {
        assert!(ask_user_callback_status_is_retryable(
            reqwest::StatusCode::BAD_GATEWAY
        ));
        assert!(ask_user_callback_status_is_retryable(
            reqwest::StatusCode::REQUEST_TIMEOUT
        ));
        assert!(ask_user_callback_status_is_retryable(
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(!ask_user_callback_status_is_retryable(
            reqwest::StatusCode::BAD_REQUEST
        ));
        assert!(!ask_user_callback_status_is_retryable(
            reqwest::StatusCode::UNAUTHORIZED
        ));
    }
}
