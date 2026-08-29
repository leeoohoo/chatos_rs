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

use crate::core::auth::AuthUser;
use crate::models::pet_activity_inbox::PetActivityInboxStatus;
use crate::repositories::pet_activity_inbox::{
    list_pet_activities, mark_pet_activities_displayed, transition_pet_activity,
};
use crate::services::pet_activity_inbox::hydrate_missing_chat_result_details;
use crate::services::realtime::publish_pet_activity_inbox_updated;

pub fn router() -> Router {
    Router::new()
        .route("/api/pet-activities", get(list_activities))
        .route(
            "/api/pet-activities/{activity_id}/displayed",
            post(mark_displayed),
        )
        .route(
            "/api/pet-activities/{activity_id}/acknowledge",
            post(acknowledge),
        )
        .route("/api/pet-activities/{activity_id}/ignore", post(ignore))
        .route(
            "/api/pet-activities/{activity_id}/handled",
            post(mark_handled),
        )
}

#[derive(Debug, Deserialize)]
struct PetActivityListQuery {
    #[serde(default)]
    include_closed: bool,
    #[serde(default = "default_true")]
    mark_displayed: bool,
    limit: Option<i64>,
}

async fn list_activities(
    auth: AuthUser,
    Query(query): Query<PetActivityListQuery>,
) -> (StatusCode, Json<Value>) {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    match list_pet_activities(&auth.user_id, query.include_closed, limit).await {
        Ok(mut activities) => {
            hydrate_missing_chat_result_details(&auth.user_id, &mut activities).await;
            if query.mark_displayed {
                let unread_ids: Vec<String> = activities
                    .iter()
                    .filter(|activity| activity.inbox_status == PetActivityInboxStatus::Unread)
                    .map(|activity| activity.id.clone())
                    .collect();
                if let Err(err) = mark_pet_activities_displayed(&auth.user_id, &unread_ids).await {
                    return server_error(err);
                }
                for activity in &mut activities {
                    if activity.inbox_status == PetActivityInboxStatus::Unread {
                        activity.inbox_status = PetActivityInboxStatus::Displayed;
                        activity.displayed_at = Some(crate::core::time::now_rfc3339());
                    }
                }
            }
            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "count": activities.len(),
                    "activities": activities,
                })),
            )
        }
        Err(err) => server_error(err),
    }
}

async fn mark_displayed(
    auth: AuthUser,
    Path(activity_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    transition(
        &auth.user_id,
        &activity_id,
        PetActivityInboxStatus::Displayed,
    )
    .await
}

async fn acknowledge(auth: AuthUser, Path(activity_id): Path<String>) -> (StatusCode, Json<Value>) {
    transition(
        &auth.user_id,
        &activity_id,
        PetActivityInboxStatus::Acknowledged,
    )
    .await
}

async fn ignore(auth: AuthUser, Path(activity_id): Path<String>) -> (StatusCode, Json<Value>) {
    transition(&auth.user_id, &activity_id, PetActivityInboxStatus::Ignored).await
}

async fn mark_handled(
    auth: AuthUser,
    Path(activity_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    transition(&auth.user_id, &activity_id, PetActivityInboxStatus::Handled).await
}

async fn transition(
    user_id: &str,
    activity_id: &str,
    status: PetActivityInboxStatus,
) -> (StatusCode, Json<Value>) {
    let activity_id = activity_id.trim();
    if activity_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": "activity_id is required" })),
        );
    }
    match transition_pet_activity(user_id, activity_id, status).await {
        Ok(Some(activity)) => {
            publish_pet_activity_inbox_updated(user_id, status.as_str(), &activity);
            (
                StatusCode::OK,
                Json(json!({ "success": true, "activity": activity })),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": "pet activity not found or already closed",
            })),
        ),
        Err(err) => server_error(err),
    }
}

fn server_error(error: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "success": false, "error": error })),
    )
}

fn default_true() -> bool {
    true
}
