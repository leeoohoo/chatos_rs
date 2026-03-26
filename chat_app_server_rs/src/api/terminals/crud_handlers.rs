use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::{normalize_non_empty, validate_existing_dir};
use crate::models::terminal::TerminalService;
use crate::models::terminal_log::TerminalLogService;
use crate::services::terminal_manager::get_terminal_manager;

use super::{attach_busy, derive_terminal_name, CreateTerminalRequest, TerminalQuery};

pub(super) async fn list_terminals(
    auth: AuthUser,
    Query(query): Query<TerminalQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let manager = get_terminal_manager();
    match TerminalService::list(Some(user_id)).await {
        Ok(list) => {
            let items = list
                .into_iter()
                .map(|t| attach_busy(&manager, t))
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(Value::Array(items)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

pub(super) async fn create_terminal(
    auth: AuthUser,
    Json(req): Json<CreateTerminalRequest>,
) -> (StatusCode, Json<Value>) {
    let CreateTerminalRequest {
        name,
        cwd,
        user_id,
        project_id,
    } = req;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let cwd = match validate_existing_dir(
        cwd.as_deref().unwrap_or(""),
        "终端目录不能为空",
        "终端目录不存在或不是目录",
    ) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err })),
            );
        }
    };

    let name = normalize_non_empty(name).unwrap_or_else(|| derive_terminal_name(&cwd));

    let manager = get_terminal_manager();
    match manager
        .create(name, cwd, Some(user_id), normalize_non_empty(project_id))
        .await
    {
        Ok(terminal) => (StatusCode::CREATED, Json(attach_busy(&manager, terminal))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

pub(super) async fn get_terminal(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let manager = get_terminal_manager();
    match ensure_owned_terminal(&id, &auth).await {
        Ok(terminal) => (StatusCode::OK, Json(attach_busy(&manager, terminal))),
        Err(err) => map_terminal_access_error(err),
    }
}

pub(super) async fn delete_terminal(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_terminal(&id, &auth).await {
        return map_terminal_access_error(err);
    }
    let manager = get_terminal_manager();
    let _ = manager.close(&id).await;
    let _ = TerminalLogService::delete_by_terminal(&id).await;
    match TerminalService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true, "message": "终端已删除" })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}
