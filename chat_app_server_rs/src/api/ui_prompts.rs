use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::services::ui_prompt_manager::{
    get_ui_prompt_payload, get_ui_prompt_record_by_id, list_pending_ui_prompt_records,
    list_ui_prompt_history_records, parse_response_submission, redact_response_for_store,
    submit_ui_prompt_response, update_ui_prompt_response, UiPromptPayload, UiPromptStatus,
    UI_PROMPT_NOT_FOUND_ERR, UI_PROMPT_TIMEOUT_MS_DEFAULT,
};

#[derive(Debug, Deserialize)]
struct PendingUiPromptQuery {
    session_id: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct UiPromptHistoryQuery {
    session_id: String,
    limit: Option<usize>,
    include_pending: Option<bool>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/ui-prompts/pending", get(list_pending_ui_prompts))
        .route("/api/ui-prompts/history", get(list_ui_prompt_history))
        .route(
            "/api/ui-prompts/:prompt_id/respond",
            post(submit_ui_prompt_response_route),
        )
}

async fn list_pending_ui_prompts(
    auth: AuthUser,
    Query(query): Query<PendingUiPromptQuery>,
) -> (StatusCode, Json<Value>) {
    if query.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "session_id is required" })),
        );
    }
    if let Err(err) = ensure_owned_session(query.session_id.as_str(), &auth).await {
        return map_session_access_error_with_success(err);
    }

    let limit = query.limit.unwrap_or(20).clamp(1, 200);
    match list_pending_ui_prompt_records(query.session_id.as_str(), limit).await {
        Ok(records) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "count": records.len(),
                "prompts": records,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn list_ui_prompt_history(
    auth: AuthUser,
    Query(query): Query<UiPromptHistoryQuery>,
) -> (StatusCode, Json<Value>) {
    if query.session_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "session_id is required" })),
        );
    }
    if let Err(err) = ensure_owned_session(query.session_id.as_str(), &auth).await {
        return map_session_access_error_with_success(err);
    }

    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let include_pending = query.include_pending.unwrap_or(false);
    match list_ui_prompt_history_records(query.session_id.as_str(), limit, include_pending).await {
        Ok(records) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "count": records.len(),
                "prompts": records,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        ),
    }
}

async fn submit_ui_prompt_response_route(
    auth: AuthUser,
    Path(prompt_id): Path<String>,
    Json(raw): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if prompt_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "prompt_id is required" })),
        );
    }

    let prompt_payload = match get_ui_prompt_payload(prompt_id.as_str()).await {
        Some(payload) => payload,
        None => {
            let fallback_record = match get_ui_prompt_record_by_id(prompt_id.as_str()).await {
                Ok(record) => record,
                Err(err) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "success": false, "error": err })),
                    )
                }
            };

            let Some(record) = fallback_record else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "success": false, "error": UI_PROMPT_NOT_FOUND_ERR })),
                );
            };

            let record_prompt = record.prompt.clone();
            let mut payload = serde_json::from_value::<UiPromptPayload>(record_prompt.clone())
                .unwrap_or_else(|_| UiPromptPayload {
                    prompt_id: record.id.clone(),
                    session_id: record.session_id.clone(),
                    conversation_turn_id: record.conversation_turn_id.clone(),
                    tool_call_id: record.tool_call_id.clone(),
                    kind: record.kind.clone(),
                    title: record_prompt
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    message: record_prompt
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    allow_cancel: record_prompt
                        .get("allow_cancel")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    timeout_ms: record_prompt
                        .get("timeout_ms")
                        .and_then(Value::as_u64)
                        .unwrap_or(UI_PROMPT_TIMEOUT_MS_DEFAULT),
                    payload: record_prompt
                        .get("payload")
                        .cloned()
                        .or_else(|| {
                            if record_prompt.is_object() {
                                Some(record_prompt.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| json!({})),
                });

            if payload.prompt_id.trim().is_empty() {
                payload.prompt_id = record.id.clone();
            }
            if payload.session_id.trim().is_empty() {
                payload.session_id = record.session_id.clone();
            }
            if payload.conversation_turn_id.trim().is_empty() {
                payload.conversation_turn_id = record.conversation_turn_id.clone();
            }
            if payload.kind.trim().is_empty() {
                payload.kind = record.kind.clone();
            }

            payload
        }
    };

    if let Err(err) = ensure_owned_session(prompt_payload.session_id.as_str(), &auth).await {
        return map_session_access_error_with_success(err);
    }

    let submission = match parse_response_submission(raw, &prompt_payload) {
        Ok(submission) => submission,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "success": false, "error": err })),
            )
        }
    };

    let resolved = match submit_ui_prompt_response(prompt_id.as_str(), submission.clone()).await {
        Ok(payload) => Some(payload),
        Err(err) if err == UI_PROMPT_NOT_FOUND_ERR || err == "ui_prompt_listener_closed" => None,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "success": false, "error": err })),
            )
        }
    };

    let status =
        UiPromptStatus::from_str(submission.status.as_str()).unwrap_or(UiPromptStatus::Canceled);
    let redacted_response = redact_response_for_store(&submission, &prompt_payload);
    if let Err(err) =
        update_ui_prompt_response(prompt_id.as_str(), status, Some(redacted_response)).await
    {
        warn!(prompt_id = %prompt_id, error = %err, "failed to persist ui prompt response");
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "prompt_id": prompt_id,
            "session_id": resolved
                .as_ref()
                .map(|payload| payload.session_id.clone())
                .unwrap_or_else(|| prompt_payload.session_id.clone()),
            "conversation_turn_id": resolved
                .as_ref()
                .map(|payload| payload.conversation_turn_id.clone())
                .unwrap_or_else(|| prompt_payload.conversation_turn_id.clone()),
            "status": submission.status,
        })),
    )
}
