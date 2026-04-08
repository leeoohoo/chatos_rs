use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::services::memory_server_client;

use super::contracts::PageQuery;

pub(super) async fn list_session_memory_summaries(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let limit = parse_positive_limit(query.limit).or(Some(20));
    let offset = parse_non_negative_offset(query.offset);

    let memory_summaries =
        match memory_server_client::list_summaries(&session_id, limit, offset).await {
            Ok(list) => list,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "获取会话总结失败", "detail": err})),
                )
            }
        };

    let total = memory_summaries.len() as i64;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "items": memory_summaries,
            "total": total,
            "has_summary": total > 0
        })),
    )
}

pub(super) async fn delete_session_memory_summary(
    auth: AuthUser,
    Path((session_id, summary_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }

    match memory_server_client::delete_summary(&session_id, &summary_id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "session_id": session_id,
                "summary_id": summary_id,
                "deleted_summaries": 1,
                "reset_messages": 0
            })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "会话总结不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "删除会话总结失败", "detail": err})),
        ),
    }
}

pub(super) async fn clear_session_memory_summaries(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }

    let deleted_count = match memory_server_client::clear_summaries(&session_id).await {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "清空会话总结失败", "detail": err})),
            )
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "session_id": session_id,
            "deleted_summaries": deleted_count,
            "reset_messages": 0
        })),
    )
}
