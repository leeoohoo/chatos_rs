// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::config::Config;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::ask_user_prompt_manager::{
    upsert_external_ask_user_prompt_record, AskUserPromptRecord, AskUserPromptStatus,
};
use crate::services::realtime::publish_sessions_updated;
use crate::services::{chatos_sessions, project_management_api_client};

mod messages;

use self::messages::{
    apply_task_runner_callback_to_user_message,
    build_task_runner_callback_assistant_message_with_contact,
    build_task_runner_callback_contact_display, is_task_runner_terminal_event,
    messages_match_for_callback_upsert, publish_task_runner_callback_realtime,
};

#[derive(Debug, Deserialize)]
pub(super) struct TaskRunnerCallbackRequest {
    event: String,
    task_id: String,
    run_id: Option<String>,
    status: String,
    task_title: String,
    #[serde(default)]
    task_objective: String,
    #[serde(default)]
    fallback_locale: String,
    project_id: Option<String>,
    task_status: Option<String>,
    result_summary: Option<String>,
    error_message: Option<String>,
    report_content: Option<String>,
    source_session_id: Option<String>,
    source_turn_id: Option<String>,
    source_user_message_id: Option<String>,
    parent_task_id: Option<String>,
    source_run_id: Option<String>,
    schedule_mode: Option<String>,
    prompt: Option<TaskRunnerCallbackPrompt>,
    callback_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskRunnerCallbackPrompt {
    prompt_id: String,
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default = "default_true")]
    allow_cancel: bool,
    #[serde(default)]
    timeout_ms: u64,
    #[serde(default)]
    payload: Value,
    #[serde(default)]
    response: Option<Value>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskRunnerCallbackResponse {
    accepted: bool,
    session_id: String,
    user_message_id: String,
    event: String,
}

pub(super) async fn task_runner_callback(
    headers: HeaderMap,
    Json(payload): Json<TaskRunnerCallbackRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = verify_task_runner_callback_secret(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "accepted": false, "error": err })),
        );
    }

    let Some(user_message_id) = normalize_callback_value(payload.source_user_message_id.as_deref())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "missing source_user_message_id" })),
        );
    };
    let Some(session_id) = normalize_callback_value(payload.source_session_id.as_deref()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "missing source_session_id" })),
        );
    };

    let session = match chatos_sessions::get_session_by_id(session_id.as_str()).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "accepted": false, "error": "session not found" })),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "accepted": false, "error": err })),
            );
        }
    };
    let mut user_message = match conversation_messages::get_message_by_id_in_session(
        &session,
        user_message_id.as_str(),
    )
    .await
    {
        Ok(Some(message)) => message,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "accepted": false, "error": "user message not found" })),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "accepted": false, "error": err })),
            );
        }
    };
    if user_message.session_id != session.id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "message session mismatch" })),
        );
    }

    if is_task_runner_ask_user_prompt_event(payload.event.as_str()) {
        return handle_task_runner_ask_user_prompt_callback(
            &session,
            user_message_id.as_str(),
            payload,
        )
        .await;
    }

    let user_message_changed =
        apply_task_runner_callback_to_user_message(&mut user_message, &payload);
    let saved_user_message = if user_message_changed {
        match conversation_messages::upsert_message_in_session(&session, &user_message).await {
            Ok(message) => Some(message),
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "accepted": false, "error": err })),
                );
            }
        }
    } else {
        Some(user_message.clone())
    };

    if let Some(saved_user_message) = saved_user_message.as_ref() {
        if let Err(err) =
            sync_project_requirement_execution_status(saved_user_message, &payload).await
        {
            warn!(
                session_id = session.id.as_str(),
                user_message_id = user_message_id.as_str(),
                task_id = payload.task_id.as_str(),
                event = payload.event.as_str(),
                error = err.as_str(),
                "failed to sync project requirement execution status"
            );
        }
    }

    let (saved_assistant_message, assistant_message_changed) = if is_task_runner_terminal_event(
        payload.event.as_str(),
    ) {
        let contact_display = build_task_runner_callback_contact_display(&session);
        let assistant_message = build_task_runner_callback_assistant_message_with_contact(
            &session.id,
            &payload,
            Some(&contact_display),
        );
        match conversation_messages::get_message_by_id_in_session(
            &session,
            assistant_message.id.as_str(),
        )
        .await
        {
            Ok(Some(existing_message)) if existing_message.session_id != session.id => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(
                        json!({ "accepted": false, "error": "assistant message session mismatch" }),
                    ),
                );
            }
            Ok(Some(existing_message))
                if messages_match_for_callback_upsert(&existing_message, &assistant_message) =>
            {
                (Some(existing_message), false)
            }
            Ok(_) => {
                match conversation_messages::upsert_message_in_session(&session, &assistant_message)
                    .await
                {
                    Ok(message) => (Some(message), true),
                    Err(err) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "accepted": false, "error": err })),
                        );
                    }
                }
            }
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "accepted": false, "error": err })),
                );
            }
        }
    } else {
        (None, false)
    };

    let session_changed = user_message_changed || assistant_message_changed;
    let refreshed_session = if session_changed {
        chatos_sessions::get_session_by_id(session.id.as_str())
            .await
            .ok()
            .flatten()
    } else {
        Some(session.clone())
    };

    let realtime_user_id = refreshed_session
        .as_ref()
        .and_then(|value| value.user_id.clone())
        .or_else(|| session.user_id.clone());
    let realtime_session_id = refreshed_session
        .as_ref()
        .map(|value| value.id.clone())
        .unwrap_or_else(|| session.id.clone());
    let realtime_project_id = refreshed_session
        .as_ref()
        .and_then(|value| value.project_id.clone())
        .or_else(|| session.project_id.clone());

    if let Some(user_id) = realtime_user_id.as_deref() {
        let callback_session = refreshed_session.as_ref().unwrap_or(&session);
        publish_task_runner_callback_realtime(
            user_id,
            callback_session,
            payload.source_turn_id.as_deref(),
            user_message_id.as_str(),
            payload.event.as_str(),
            saved_user_message.as_ref(),
            saved_assistant_message.as_ref(),
        );
        if session_changed {
            publish_sessions_updated(
                user_id,
                "task_runner_callback",
                Some(realtime_session_id.as_str()),
                realtime_project_id.as_deref(),
                refreshed_session,
            );
        }
    } else {
        warn!(
            session_id = realtime_session_id.as_str(),
            task_id = payload.task_id.as_str(),
            event = payload.event.as_str(),
            "task runner callback persisted without realtime user id; skipped realtime publish"
        );
    }

    info!(
        session_id = session_id.as_str(),
        user_message_id = user_message_id.as_str(),
        task_id = payload.task_id.as_str(),
        run_id = payload.run_id.as_deref().unwrap_or_default(),
        event = payload.event.as_str(),
        user_message_changed,
        assistant_message_changed,
        "accepted task runner callback"
    );

    (
        StatusCode::OK,
        Json(json!(TaskRunnerCallbackResponse {
            accepted: true,
            session_id,
            user_message_id,
            event: payload.event,
        })),
    )
}

async fn handle_task_runner_ask_user_prompt_callback(
    session: &Session,
    user_message_id: &str,
    payload: TaskRunnerCallbackRequest,
) -> (StatusCode, Json<Value>) {
    let Some(prompt) = payload.prompt.clone() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "missing ask user prompt payload" })),
        );
    };
    let prompt_id = prompt.prompt_id.trim();
    if prompt_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "accepted": false, "error": "missing prompt_id" })),
        );
    }
    let turn_id = normalize_callback_value(payload.source_turn_id.as_deref())
        .unwrap_or_else(|| user_message_id.to_string());
    let status = ask_user_prompt_status_from_task_runner_event(payload.event.as_str(), &prompt);
    let now = normalize_callback_value(payload.callback_at.as_deref())
        .unwrap_or_else(crate::core::time::now_rfc3339);
    let response = prompt.response.clone();
    let prompt_kind = normalize_callback_value(Some(prompt.kind.as_str()))
        .unwrap_or_else(|| "task_runner_ask_user_prompt".to_string());
    let prompt_title = prompt.title.clone();
    let prompt_message = prompt.message.clone();
    let prompt_payload = prompt.payload.clone();
    let expires_at = prompt.expires_at.clone();
    let record = AskUserPromptRecord {
        id: prompt_id.to_string(),
        conversation_id: session.id.clone(),
        conversation_turn_id: turn_id,
        tool_call_id: None,
        kind: prompt_kind.clone(),
        status,
        prompt: json!({
            "prompt_id": prompt_id,
            "conversation_id": session.id,
            "conversation_turn_id": payload.source_turn_id,
            "kind": prompt_kind,
            "title": prompt_title,
            "message": prompt_message,
            "allow_cancel": prompt.allow_cancel,
            "timeout_ms": prompt.timeout_ms,
            "payload": prompt_payload,
            "source": "task_runner",
            "external_task_id": payload.task_id,
            "external_run_id": payload.run_id,
            "external_project_id": payload.project_id,
            "task_status": payload.task_status,
        }),
        response,
        expires_at,
        source: "task_runner".to_string(),
        external_prompt_id: Some(prompt_id.to_string()),
        external_task_id: Some(payload.task_id.clone()),
        external_run_id: payload.run_id.clone(),
        external_project_id: payload.project_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };

    let saved = match upsert_external_ask_user_prompt_record(record).await {
        Ok(record) => record,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "accepted": false, "error": err })),
            );
        }
    };

    info!(
        session_id = session.id.as_str(),
        user_message_id,
        task_id = payload.task_id.as_str(),
        run_id = payload.run_id.as_deref().unwrap_or_default(),
        prompt_id = saved.id.as_str(),
        event = payload.event.as_str(),
        "accepted task runner ask user prompt callback"
    );

    (
        StatusCode::OK,
        Json(json!({
            "accepted": true,
            "session_id": session.id,
            "user_message_id": user_message_id,
            "event": payload.event,
            "prompt_id": saved.id,
            "status": saved.status.as_str(),
        })),
    )
}

fn verify_task_runner_callback_secret(headers: &HeaderMap) -> Result<(), String> {
    let expected = Config::try_get()
        .ok()
        .and_then(|config| config.task_runner_callback_secret.clone());
    let Some(expected) = expected.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let actual = headers
        .get("x-task-runner-callback-secret")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing task runner callback secret".to_string())?;
    if actual == expected {
        Ok(())
    } else {
        Err("invalid task runner callback secret".to_string())
    }
}

async fn sync_project_requirement_execution_status(
    message: &Message,
    payload: &TaskRunnerCallbackRequest,
) -> Result<(), String> {
    if !is_project_requirement_execution_message(message)
        || normalize_callback_value(payload.parent_task_id.as_deref()).is_some()
    {
        return Ok(());
    }
    if should_ignore_stopped_task_cancel_callback(message.metadata.as_ref(), payload) {
        return Ok(());
    }
    let cfg = Config::try_get().map_err(|err| err.to_string())?;
    let Some(sync_secret) = cfg
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err("project service sync secret is not configured".to_string());
    };
    let task_runner_status = payload
        .task_status
        .clone()
        .or_else(|| Some(payload.status.clone()));
    project_management_api_client::sync_task_runner_task_status(
        cfg.project_service_base_url.as_str(),
        sync_secret,
        payload.task_id.as_str(),
        &project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
            task_runner_task_id: payload.task_id.clone(),
            task_runner_run_id: payload.run_id.clone(),
            task_runner_status,
            execution_group_id: payload.source_user_message_id.clone(),
            last_callback_event: Some(payload.event.clone()),
            last_callback_at: payload.callback_at.clone(),
            last_error_message: payload.error_message.clone(),
            source_session_id: payload.source_session_id.clone(),
            source_user_message_id: payload.source_user_message_id.clone(),
        },
    )
    .await
    .map(|_| ())
}

fn is_project_requirement_execution_message(message: &Message) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("project_requirement_execution"))
        .is_some_and(Value::is_object)
}

fn should_ignore_stopped_task_cancel_callback(
    metadata: Option<&Value>,
    payload: &TaskRunnerCallbackRequest,
) -> bool {
    if payload.event != "task.cancelled" {
        return false;
    }
    let task_id = payload.task_id.trim();
    if task_id.is_empty() {
        return false;
    }
    metadata
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("stopped_task_ids"))
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .any(|value| value.trim() == task_id)
        })
}

fn normalize_callback_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn is_task_runner_ask_user_prompt_event(event: &str) -> bool {
    matches!(
        event,
        "ask_user_prompt.required" | "ask_user_prompt.resolved"
    )
}

fn ask_user_prompt_status_from_task_runner_event(
    event: &str,
    prompt: &TaskRunnerCallbackPrompt,
) -> AskUserPromptStatus {
    if event == "ask_user_prompt.required" {
        return AskUserPromptStatus::Pending;
    }
    prompt
        .status
        .as_deref()
        .and_then(AskUserPromptStatus::from_str)
        .unwrap_or(AskUserPromptStatus::Ok)
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_requirement_execution_sync_only_applies_to_execution_messages() {
        let mut execution_message = Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execute".to_string(),
        );
        execution_message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            }
        }));
        let ordinary_message = Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "ordinary task".to_string(),
        );

        assert!(is_project_requirement_execution_message(&execution_message));
        assert!(!is_project_requirement_execution_message(&ordinary_message));
    }
}
