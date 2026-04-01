use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;
use std::path::Path as FsPath;

use crate::core::auth::AuthUser;
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::{normalize_non_empty, validate_existing_dir};
use crate::models::terminal::TerminalService;
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::project_run::validate_command_preflight;
use crate::services::terminal_manager::get_terminal_manager;

use super::contracts::InterruptTerminalRequest;
use super::{
    attach_busy, derive_terminal_name, CreateTerminalRequest, DispatchTerminalCommandRequest, TerminalQuery,
};

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

fn normalized_cwd(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(&['/', '\\'][..]);
    if trimmed.is_empty() {
        path.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_same_cwd(left: &str, right: &str) -> bool {
    normalized_cwd(left) == normalized_cwd(right)
}

fn terminal_name_from_cwd(cwd: &str) -> String {
    let trimmed = cwd.trim().trim_end_matches(&['/', '\\'][..]);
    if trimmed.is_empty() {
        return "Terminal".to_string();
    }
    FsPath::new(trimmed)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| derive_terminal_name(trimmed))
}

pub(super) async fn dispatch_terminal_command(
    auth: AuthUser,
    Json(req): Json<DispatchTerminalCommandRequest>,
) -> (StatusCode, Json<Value>) {
    let DispatchTerminalCommandRequest {
        cwd,
        command,
        user_id,
        project_id,
        create_if_missing,
    } = req;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let cwd = match validate_existing_dir(
        cwd.as_deref().unwrap_or(""),
        "运行目录不能为空",
        "运行目录不存在或不是目录",
    ) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err })),
            );
        }
    };
    let command = match normalize_non_empty(command) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "运行命令不能为空" })),
            );
        }
    };
    if let Err(err) = validate_command_preflight(command.as_str(), cwd.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": err })));
    }
    let normalized_project_id = normalize_non_empty(project_id);
    let allow_create = create_if_missing.unwrap_or(true);

    let manager = get_terminal_manager();
    let mut terminals = match TerminalService::list(Some(user_id.clone())).await {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err })),
            );
        }
    };

    terminals.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    let reusable = terminals.into_iter().find(|terminal| {
        if terminal.status != "running" {
            return false;
        }
        if !is_same_cwd(terminal.cwd.as_str(), cwd.as_str()) {
            return false;
        }
        if let Some(project_id) = normalized_project_id.as_deref() {
            if terminal.project_id.as_deref() != Some(project_id) {
                return false;
            }
        }
        !manager.get_busy(terminal.id.as_str()).unwrap_or(false)
    });

    let (terminal, reused) = if let Some(terminal) = reusable {
        (terminal, true)
    } else if allow_create {
        let name = terminal_name_from_cwd(cwd.as_str());
        match manager
            .create(
                name,
                cwd.clone(),
                Some(user_id.clone()),
                normalized_project_id.clone(),
            )
            .await
        {
            Ok(terminal) => (terminal, false),
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": err })),
                );
            }
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "未找到可复用终端，且未允许自动创建" })),
        );
    };

    let session = match manager.ensure_running(&terminal).await {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err })),
            );
        }
    };

    let input = format!("{command}\n");
    if let Err(err) = session.write_input(input.as_str()) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        );
    }

    let cmd_log = TerminalLog::new(
        terminal.id.clone(),
        "command".to_string(),
        command.clone(),
    );
    let input_log = TerminalLog::new(terminal.id.clone(), "input".to_string(), input.clone());
    let _ = TerminalLogService::create(cmd_log).await;
    let _ = TerminalLogService::create(input_log).await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "terminal_id": terminal.id,
            "terminal_name": terminal.name,
            "terminal_reused": reused,
            "cwd": terminal.cwd,
            "executed_command": command,
        })),
    )
}

pub(super) async fn interrupt_terminal_command(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<InterruptTerminalRequest>,
) -> (StatusCode, Json<Value>) {
    let terminal = match ensure_owned_terminal(&id, &auth).await {
        Ok(terminal) => terminal,
        Err(err) => return map_terminal_access_error(err),
    };
    let manager = get_terminal_manager();
    let session = match manager.ensure_running(&terminal).await {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err })),
            );
        }
    };
    if let Err(err) = session.write_input("\u{3}") {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        );
    }
    let reason = normalize_non_empty(req.reason).unwrap_or_else(|| "manual_interrupt".to_string());
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "signal".to_string(),
        format!("ctrl_c:{reason}"),
    ))
    .await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "terminal_id": terminal.id,
            "terminal_name": terminal.name,
            "interrupted": true,
            "signal": "SIGINT",
            "reason": reason,
        })),
    )
}
