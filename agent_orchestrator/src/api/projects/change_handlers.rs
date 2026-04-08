use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;
use std::collections::HashSet;

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::repositories::change_logs;

use super::change_support::{build_change_paths, collect_change_ids_for_paths};
use super::contracts::{ConfirmProjectChangesRequest, ProjectChangeQuery};

pub(super) async fn list_project_changes(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<ProjectChangeQuery>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };

    let paths = build_change_paths(&project, query.path);
    let limit = query.limit.or(Some(100));
    let offset = query.offset.unwrap_or(0);

    match change_logs::list_project_change_logs(&project.id, paths, limit, offset).await {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn get_project_change_summary(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };

    match change_logs::list_unconfirmed_project_changes(&project.id, &project.root_path).await {
        Ok(records) => {
            let summary = change_logs::summarize_project_changes(&records);
            (
                StatusCode::OK,
                Json(serde_json::to_value(summary).unwrap_or(Value::Null)),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn confirm_project_changes(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ConfirmProjectChangesRequest>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };

    let records = match change_logs::list_unconfirmed_project_changes(
        &project.id,
        &project.root_path,
    )
    .await
    {
        Ok(records) => records,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err})),
            )
        }
    };

    let mode = req
        .mode
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if req
                .change_ids
                .as_ref()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
            {
                Some("change_ids".to_string())
            } else if req
                .paths
                .as_ref()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
            {
                Some("paths".to_string())
            } else {
                Some("all".to_string())
            }
        })
        .unwrap_or_else(|| "all".to_string());

    let candidate_ids: Vec<String> = match mode.as_str() {
        "all" => records.iter().map(|item| item.id.clone()).collect(),
        "paths" => {
            let paths = req.paths.unwrap_or_default();
            if paths.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "paths 不能为空"})),
                );
            }
            collect_change_ids_for_paths(&records, &project.root_path, &paths)
        }
        "change_ids" => {
            let requested = req.change_ids.unwrap_or_default();
            if requested.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "change_ids 不能为空"})),
                );
            }
            let allowed_ids: HashSet<String> = records.iter().map(|item| item.id.clone()).collect();
            requested
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty() && allowed_ids.contains(value))
                .collect()
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "mode 仅支持 all / paths / change_ids"})),
            )
        }
    };

    match change_logs::confirm_change_logs_by_ids(&candidate_ids, Some(auth.user_id.as_str())).await
    {
        Ok(confirmed) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "confirmed": confirmed,
                "requested": candidate_ids.len(),
                "mode": mode,
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
