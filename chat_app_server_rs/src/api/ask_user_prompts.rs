// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::services::access_token_scope;
use crate::services::ask_user_prompt_manager::{
    get_ask_user_prompt_record, list_ask_user_prompt_history_records, redact_response_for_store,
    submit_ask_user_prompt_response, update_ask_user_prompt_response, AskUserPromptPayload,
    AskUserPromptRecord, AskUserPromptResponseSubmission, AskUserPromptStatus,
};
use crate::services::task_runner_api_client::{
    cancel_task_runner_prompt, submit_task_runner_prompt, CancelTaskRunnerPromptRequest,
    SubmitTaskRunnerPromptRequest,
};

pub fn router() -> Router {
    Router::new()
        .route("/api/ask-user-prompts", get(list_ask_user_prompts))
        .route(
            "/api/ask-user-prompts/{prompt_id}/submit",
            post(submit_ask_user_prompt),
        )
        .route(
            "/api/ask-user-prompts/{prompt_id}/cancel",
            post(cancel_ask_user_prompt),
        )
}

#[derive(Debug, Deserialize)]
struct AskUserPromptListQuery {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: String,
    include_pending: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct SubmitAskUserPromptApiRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: Option<String>,
    status: Option<String>,
    values: Option<Value>,
    selection: Option<Value>,
    reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CancelAskUserPromptApiRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: Option<String>,
    reason: Option<String>,
}

async fn list_ask_user_prompts(
    auth: AuthUser,
    Query(query): Query<AskUserPromptListQuery>,
) -> (StatusCode, Json<Value>) {
    let conversation_id = query.conversation_id.trim();
    if conversation_id.is_empty() {
        return bad_request("conversation_id is required");
    }
    if let Err(err) = ensure_owned_session(conversation_id, &auth).await {
        return map_session_access_error_with_success(err);
    }

    let include_pending = query.include_pending.unwrap_or(true);
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    match list_ask_user_prompt_history_records(conversation_id, limit, include_pending).await {
        Ok(prompts) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "count": prompts.len(),
                "prompts": prompts,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn submit_ask_user_prompt(
    auth: AuthUser,
    Path(prompt_id): Path<String>,
    Json(req): Json<SubmitAskUserPromptApiRequest>,
) -> (StatusCode, Json<Value>) {
    let Some(record) =
        load_authorized_prompt(prompt_id.as_str(), req.conversation_id.as_deref(), &auth).await
    else {
        return not_found("ask user prompt not found");
    };
    if record.status != AskUserPromptStatus::Pending {
        return bad_request("ask user prompt is already resolved");
    }

    let submission = AskUserPromptResponseSubmission {
        status: req
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("ok")
            .to_string(),
        values: req.values,
        selection: req.selection,
        reason: req.reason,
    };
    let next_status = AskUserPromptStatus::from_str(submission.status.as_str())
        .unwrap_or(AskUserPromptStatus::Ok);
    if matches!(next_status, AskUserPromptStatus::Pending) {
        return bad_request("status must not be pending");
    }
    if matches!(next_status, AskUserPromptStatus::Canceled) {
        return cancel_ask_user_prompt_record(record, submission.reason.clone()).await;
    }

    if record.source == "task_runner" {
        return submit_task_runner_ask_user_prompt(record, submission).await;
    }
    submit_local_ask_user_prompt(record, submission, next_status).await
}

async fn cancel_ask_user_prompt(
    auth: AuthUser,
    Path(prompt_id): Path<String>,
    Json(req): Json<CancelAskUserPromptApiRequest>,
) -> (StatusCode, Json<Value>) {
    let Some(record) =
        load_authorized_prompt(prompt_id.as_str(), req.conversation_id.as_deref(), &auth).await
    else {
        return not_found("ask user prompt not found");
    };
    if record.status != AskUserPromptStatus::Pending {
        return bad_request("ask user prompt is already resolved");
    }
    cancel_ask_user_prompt_record(record, req.reason).await
}

async fn submit_local_ask_user_prompt(
    record: AskUserPromptRecord,
    submission: AskUserPromptResponseSubmission,
    next_status: AskUserPromptStatus,
) -> (StatusCode, Json<Value>) {
    let payload = payload_from_record(&record);
    if matches!(next_status, AskUserPromptStatus::Canceled) && !payload.allow_cancel {
        return bad_request("cancel is not allowed for this prompt");
    }
    if let Err(err) = submit_ask_user_prompt_response(record.id.as_str(), submission.clone()).await
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        );
    }
    let redacted_response = redact_response_for_store(&submission, &payload);
    match update_ask_user_prompt_response(record.id.as_str(), next_status, Some(redacted_response))
        .await
    {
        Ok(saved) => ok_prompt(saved),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn submit_task_runner_ask_user_prompt(
    record: AskUserPromptRecord,
    submission: AskUserPromptResponseSubmission,
) -> (StatusCode, Json<Value>) {
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "success": false, "error": "missing access token" })),
        );
    };
    let external_prompt_id = record
        .external_prompt_id
        .as_deref()
        .unwrap_or(record.id.as_str())
        .trim()
        .to_string();
    if external_prompt_id.is_empty() {
        return bad_request("external_prompt_id is required");
    }

    let request = SubmitTaskRunnerPromptRequest {
        values: submission.values.clone(),
        selection: submission.selection.clone(),
        reason: submission.reason.clone(),
    };
    match submit_task_runner_prompt(
        Config::get().task_runner_base_url.as_str(),
        access_token.as_str(),
        external_prompt_id.as_str(),
        &request,
    )
    .await
    {
        Ok(remote_prompt) => {
            let payload = payload_from_record(&record);
            let redacted_response = redact_response_for_store(&submission, &payload);
            match update_ask_user_prompt_response(
                record.id.as_str(),
                AskUserPromptStatus::Ok,
                Some(redacted_response),
            )
            .await
            {
                Ok(saved) => ok_prompt_with_remote(saved, remote_prompt),
                Err(err) => (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "success": false, "error": err })),
                ),
            }
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn cancel_ask_user_prompt_record(
    record: AskUserPromptRecord,
    reason: Option<String>,
) -> (StatusCode, Json<Value>) {
    let payload = payload_from_record(&record);
    if !payload.allow_cancel {
        return bad_request("cancel is not allowed for this prompt");
    }
    if record.source == "task_runner" {
        return cancel_task_runner_ask_user_prompt(record, reason).await;
    }

    let submission = AskUserPromptResponseSubmission {
        status: AskUserPromptStatus::Canceled.as_str().to_string(),
        values: None,
        selection: None,
        reason,
    };
    if let Err(err) = submit_ask_user_prompt_response(record.id.as_str(), submission.clone()).await
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        );
    }
    let redacted_response = redact_response_for_store(&submission, &payload);
    match update_ask_user_prompt_response(
        record.id.as_str(),
        AskUserPromptStatus::Canceled,
        Some(redacted_response),
    )
    .await
    {
        Ok(saved) => ok_prompt(saved),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn cancel_task_runner_ask_user_prompt(
    record: AskUserPromptRecord,
    reason: Option<String>,
) -> (StatusCode, Json<Value>) {
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "success": false, "error": "missing access token" })),
        );
    };
    let external_prompt_id = record
        .external_prompt_id
        .as_deref()
        .unwrap_or(record.id.as_str())
        .trim()
        .to_string();
    if external_prompt_id.is_empty() {
        return bad_request("external_prompt_id is required");
    }
    let request = CancelTaskRunnerPromptRequest {
        reason: reason.clone(),
    };
    match cancel_task_runner_prompt(
        Config::get().task_runner_base_url.as_str(),
        access_token.as_str(),
        external_prompt_id.as_str(),
        &request,
    )
    .await
    {
        Ok(remote_prompt) => {
            let payload = payload_from_record(&record);
            let submission = AskUserPromptResponseSubmission {
                status: AskUserPromptStatus::Canceled.as_str().to_string(),
                values: None,
                selection: None,
                reason,
            };
            let redacted_response = redact_response_for_store(&submission, &payload);
            match update_ask_user_prompt_response(
                record.id.as_str(),
                AskUserPromptStatus::Canceled,
                Some(redacted_response),
            )
            .await
            {
                Ok(saved) => ok_prompt_with_remote(saved, remote_prompt),
                Err(err) => (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "success": false, "error": err })),
                ),
            }
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn load_authorized_prompt(
    prompt_id: &str,
    requested_conversation_id: Option<&str>,
    auth: &AuthUser,
) -> Option<AskUserPromptRecord> {
    let record = match get_ask_user_prompt_record(prompt_id).await {
        Ok(record) => record?,
        Err(_) => return None,
    };
    let requested_conversation_id = requested_conversation_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if requested_conversation_id
        .is_some_and(|conversation_id| conversation_id != record.conversation_id)
    {
        return None;
    }
    if ensure_owned_session(record.conversation_id.as_str(), auth)
        .await
        .is_err()
    {
        return None;
    }
    Some(record)
}

fn payload_from_record(record: &AskUserPromptRecord) -> AskUserPromptPayload {
    if let Ok(payload) = serde_json::from_value::<AskUserPromptPayload>(record.prompt.clone()) {
        return payload;
    }
    let payload = record
        .prompt
        .get("payload")
        .cloned()
        .unwrap_or_else(|| json!({}));
    AskUserPromptPayload {
        prompt_id: record.id.clone(),
        conversation_id: record.conversation_id.clone(),
        conversation_turn_id: record.conversation_turn_id.clone(),
        tool_call_id: record.tool_call_id.clone(),
        kind: record.kind.clone(),
        title: record
            .prompt
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        message: record
            .prompt
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        allow_cancel: record
            .prompt
            .get("allow_cancel")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timeout_ms: record
            .prompt
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(
                crate::services::ask_user_prompt_manager::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
            ),
        payload,
    }
}

fn ok_prompt(record: AskUserPromptRecord) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({ "success": true, "prompt": record })),
    )
}

fn ok_prompt_with_remote(
    record: AskUserPromptRecord,
    remote_prompt: Value,
) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "prompt": record,
            "task_runner_prompt": remote_prompt,
        })),
    )
}

fn bad_request(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "success": false, "error": message.into() })),
    )
}

fn not_found(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "success": false, "error": message.into() })),
    )
}
