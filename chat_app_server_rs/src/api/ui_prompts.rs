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
    get_ui_prompt_payload, list_pending_ui_prompt_records, parse_response_submission,
    redact_response_for_store, submit_ui_prompt_response, update_ui_prompt_response,
    UiPromptStatus, UI_PROMPT_NOT_FOUND_ERR,
};

#[derive(Debug, Deserialize)]
struct PendingUiPromptQuery {
    session_id: String,
    limit: Option<usize>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/ui-prompts/pending", get(list_pending_ui_prompts))
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
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "success": false, "error": UI_PROMPT_NOT_FOUND_ERR })),
            )
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
        Ok(payload) => payload,
        Err(err) if err == UI_PROMPT_NOT_FOUND_ERR => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "success": false, "error": err })),
            )
        }
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
            "session_id": resolved.session_id,
            "conversation_turn_id": resolved.conversation_turn_id,
            "status": submission.status,
        })),
    )
}
