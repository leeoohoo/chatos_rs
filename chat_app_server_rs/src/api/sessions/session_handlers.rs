use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::normalize_non_empty;
use crate::repositories::projects as projects_repo;
use crate::services::memory_server_client;

use super::contracts::{CreateSessionRequest, SessionQuery, UpdateSessionRequest};
use super::support::{
    contact_agent_id_from_metadata, contact_id_from_metadata, normalize_project_scope,
};

pub(super) async fn list_sessions(
    auth: AuthUser,
    Query(query): Query<SessionQuery>,
) -> (StatusCode, Json<Value>) {
    let SessionQuery {
        user_id,
        project_id,
        limit,
        offset,
        include_archived,
        include_archiving,
    } = query;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let limit = parse_positive_limit(limit);
    let offset = parse_non_negative_offset(offset);
    let include_archived = include_archived
        .as_deref()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);
    let include_archiving = include_archiving
        .as_deref()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);

    let result = memory_server_client::list_sessions(
        Some(user_id.as_str()),
        project_id.as_deref(),
        limit,
        offset,
        include_archived,
        include_archiving,
    )
    .await;
    match result {
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

pub(super) async fn create_session(
    auth: AuthUser,
    Json(req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let CreateSessionRequest {
        title,
        description,
        metadata,
        user_id,
        project_id,
    } = req;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let Some(title) = normalize_non_empty(title) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "对话线程标题不能为空"})),
        );
    };

    let _ = description;
    match memory_server_client::create_session(user_id.clone(), title, project_id, metadata).await {
        Ok(saved) => {
            let metadata = saved.metadata.as_ref();
            let project_id = normalize_project_scope(saved.project_id.as_deref());
            if project_id != "0" {
                if let Ok(Some(project)) =
                    projects_repo::get_project_by_id(project_id.as_str()).await
                {
                    let same_owner = project
                        .user_id
                        .as_deref()
                        .map(|owner| owner == user_id.as_str())
                        .unwrap_or(true);
                    if same_owner {
                        if let Err(err) = memory_server_client::sync_memory_project(
                            &memory_server_client::SyncMemoryProjectRequestDto {
                                user_id: Some(user_id.clone()),
                                project_id: Some(project.id.clone()),
                                name: Some(project.name.clone()),
                                root_path: Some(project.root_path.clone()),
                                description: project.description.clone(),
                                status: Some("active".to_string()),
                                is_virtual: Some(false),
                            },
                        )
                        .await
                        {
                            eprintln!(
                                "[SESSIONS] sync memory project failed while creating session: project_id={} err={}",
                                project.id, err
                            );
                        }
                    }
                }
            } else if let Err(err) = memory_server_client::sync_memory_project(
                &memory_server_client::SyncMemoryProjectRequestDto {
                    user_id: Some(user_id.clone()),
                    project_id: Some("0".to_string()),
                    name: Some("未指定项目".to_string()),
                    root_path: None,
                    description: None,
                    status: Some("active".to_string()),
                    is_virtual: Some(true),
                },
            )
            .await
            {
                eprintln!(
                    "[SESSIONS] sync virtual memory project failed while creating session: err={}",
                    err
                );
            }
            if let Some(agent_id) = contact_agent_id_from_metadata(metadata) {
                if let Err(err) = memory_server_client::sync_project_agent_link(
                    &memory_server_client::SyncProjectAgentLinkRequestDto {
                        user_id: Some(user_id.clone()),
                        project_id: Some(project_id),
                        agent_id: Some(agent_id),
                        contact_id: contact_id_from_metadata(metadata),
                        session_id: Some(saved.id.clone()),
                        last_message_at: None,
                        status: Some("active".to_string()),
                    },
                )
                .await
                {
                    eprintln!(
                        "[SESSIONS] sync project-agent link failed: session_id={} err={}",
                        saved.id, err
                    );
                }
            }
            (
                StatusCode::CREATED,
                Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn get_session(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_session(&id, &auth).await {
        Ok(session) => (
            StatusCode::OK,
            Json(serde_json::to_value(session).unwrap_or(Value::Null)),
        ),
        Err(err) => map_session_access_error(err),
    }
}

pub(super) async fn update_session(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&id, &auth).await {
        return map_session_access_error(err);
    }

    let _ = req.description;
    match memory_server_client::update_session(&id, req.title.clone(), None, req.metadata.clone())
        .await
    {
        Ok(Some(session)) => (
            StatusCode::OK,
            Json(serde_json::to_value(session).unwrap_or(Value::Null)),
        ),
        Ok(None) => (StatusCode::OK, Json(Value::Null)),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn delete_session(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&id, &auth).await {
        return map_session_access_error(err);
    }

    match memory_server_client::delete_session(&id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "message": "对话线程已归档"})),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "对话线程不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
