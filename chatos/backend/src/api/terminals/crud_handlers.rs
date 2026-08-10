// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::api::local_connectors::{
    close_local_terminal_session, create_local_terminal_session, parse_local_connector_root_path,
    send_local_terminal_input, validate_local_connector_workspace_ref, LocalConnectorRootRef,
};
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::terminal_access::{ensure_owned_terminal, map_terminal_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::user_visible_path::display_path;
use crate::core::validation::normalize_non_empty;
use crate::models::terminal::{Terminal, TerminalService, TERMINAL_KIND_SHARED};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::realtime::{
    publish_terminal_list_invalidated, publish_terminal_state_changed,
};

use super::contracts::InterruptTerminalRequest;
use super::{
    derive_terminal_name, terminal_response, CreateTerminalRequest, DispatchTerminalCommandRequest,
    TerminalQuery,
};

fn connector_error_message(err: (StatusCode, Json<Value>)) -> String {
    let (status, Json(value)) = err;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(|message| format!("{message} ({status})"))
        .unwrap_or_else(|| format!("{value} ({status})"))
}

async fn require_local_connector_root(
    raw: &str,
) -> Result<(LocalConnectorRootRef, String), (StatusCode, Json<Value>)> {
    let root_ref = parse_local_connector_root_path(raw).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Chatos 不执行本机终端；终端目录必须来自在线 Local Connector 工作区"
            })),
        )
    })?;
    let alias = validate_local_connector_workspace_ref(&root_ref).await?;
    Ok((root_ref, alias))
}

async fn ensure_local_cwd_matches_project(
    auth: &AuthUser,
    project_id: Option<&str>,
    cwd: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(project_id) = project_id else {
        return Ok(());
    };
    let project = ensure_owned_project(project_id, auth)
        .await
        .map_err(map_project_access_error)?;
    if project.root_path.trim() != cwd.trim() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Local Connector 终端必须绑定当前本地项目目录" })),
        ));
    }
    Ok(())
}

pub(super) async fn list_terminals(
    auth: AuthUser,
    Query(query): Query<TerminalQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    match TerminalService::list(Some(user_id)).await {
        Ok(list) => {
            let active_terminals = cleanup_exited_terminals(list).await;
            let items = active_terminals
                .into_iter()
                .filter(|terminal| parse_local_connector_root_path(&terminal.cwd).is_some())
                .map(terminal_response)
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(Value::Array(items)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn cleanup_exited_terminals(items: Vec<Terminal>) -> Vec<Terminal> {
    let mut active = Vec::with_capacity(items.len());
    for terminal in items {
        if terminal.status.trim().eq_ignore_ascii_case("exited") {
            let _ = TerminalLogService::delete_by_terminal(terminal.id.as_str()).await;
            let _ = TerminalService::delete(terminal.id.as_str()).await;
            if let Some(user_id) = terminal.user_id.as_deref() {
                publish_terminal_list_invalidated(
                    user_id,
                    Some(terminal.id.as_str()),
                    terminal.project_id.as_deref(),
                    "deleted",
                    None,
                );
            }
            continue;
        }
        active.push(terminal);
    }
    active
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
    let cwd = cwd.unwrap_or_default();
    let (_, alias) = match require_local_connector_root(cwd.as_str()).await {
        Ok(value) => value,
        Err(err) => return err,
    };
    let normalized_project_id = normalize_non_empty(project_id);
    if let Err(err) =
        ensure_local_cwd_matches_project(&auth, normalized_project_id.as_deref(), cwd.as_str())
            .await
    {
        return err;
    }

    let terminal_name =
        normalize_non_empty(name).unwrap_or_else(|| derive_terminal_name(alias.as_str()));
    let terminal = Terminal::new(
        terminal_name,
        cwd.trim().to_string(),
        TERMINAL_KIND_SHARED.to_string(),
        Some(user_id.clone()),
        normalized_project_id,
    );
    if let Err(err) = terminals::create_terminal(&terminal).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        );
    }
    publish_terminal_list_invalidated(
        user_id.as_str(),
        Some(terminal.id.as_str()),
        terminal.project_id.as_deref(),
        "created",
        Some(&terminal),
    );
    publish_terminal_state_changed(user_id.as_str(), &terminal, false, "created", None);
    (StatusCode::CREATED, Json(terminal_response(terminal)))
}

pub(super) async fn get_terminal(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_terminal(&id, &auth).await {
        Ok(terminal) if parse_local_connector_root_path(&terminal.cwd).is_some() => {
            (StatusCode::OK, Json(terminal_response(terminal)))
        }
        Ok(_) => (
            StatusCode::GONE,
            Json(serde_json::json!({ "error": "该终端来自已移除的 Chatos 本机终端运行时" })),
        ),
        Err(err) => map_terminal_access_error(err),
    }
}

pub(super) async fn delete_terminal(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let terminal = match ensure_owned_terminal(&id, &auth).await {
        Ok(terminal) => terminal,
        Err(err) => return map_terminal_access_error(err),
    };
    if let Some(root_ref) = parse_local_connector_root_path(&terminal.cwd) {
        if let Err(err) = close_local_terminal_session(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            terminal.id.as_str(),
        )
        .await
        {
            tracing::warn!(
                terminal_id = terminal.id.as_str(),
                error = connector_error_message(err).as_str(),
                "close Local Connector terminal before deleting metadata failed"
            );
        }
    }
    let _ = TerminalLogService::delete_by_terminal(&id).await;
    match TerminalService::delete(&id).await {
        Ok(_) => {
            if let Some(user_id) = terminal.user_id.as_deref() {
                publish_terminal_list_invalidated(
                    user_id,
                    Some(id.as_str()),
                    terminal.project_id.as_deref(),
                    "deleted",
                    None,
                );
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({ "success": true, "message": "终端已删除" })),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
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
    let cwd = cwd.unwrap_or_default();
    let (root_ref, alias) = match require_local_connector_root(cwd.as_str()).await {
        Ok(value) => value,
        Err(err) => return err,
    };
    let normalized_project_id = normalize_non_empty(project_id);
    if let Err(err) =
        ensure_local_cwd_matches_project(&auth, normalized_project_id.as_deref(), cwd.as_str())
            .await
    {
        return err;
    }
    let command = match normalize_non_empty(command) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "运行命令不能为空" })),
            );
        }
    };

    let mut candidates = match TerminalService::list(Some(user_id.clone())).await {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err })),
            );
        }
    };
    candidates.sort_by(|left, right| right.last_active_at.cmp(&left.last_active_at));
    candidates.retain(|terminal| {
        terminal.status == "running"
            && terminal.cwd.trim() == cwd.trim()
            && normalized_project_id
                .as_deref()
                .map(|project_id| terminal.project_id.as_deref() == Some(project_id))
                .unwrap_or(true)
    });

    let mut reusable = None;
    for terminal in candidates {
        let state = match create_local_terminal_session(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            terminal.id.as_str(),
            root_ref.relative_path.as_deref(),
            120,
            32,
        )
        .await
        {
            Ok(state) => state,
            Err(err) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({ "error": connector_error_message(err) })),
                );
            }
        };
        if !state.get("busy").and_then(Value::as_bool).unwrap_or(false) {
            reusable = Some(terminal);
            break;
        }
    }

    let allow_create = create_if_missing.unwrap_or(true);
    let (terminal, reused) = if let Some(terminal) = reusable {
        (terminal, true)
    } else if allow_create {
        let terminal = Terminal::new(
            derive_terminal_name(alias.as_str()),
            cwd.trim().to_string(),
            TERMINAL_KIND_SHARED.to_string(),
            Some(user_id.clone()),
            normalized_project_id,
        );
        if let Err(err) = terminals::create_terminal(&terminal).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err })),
            );
        }
        publish_terminal_list_invalidated(
            user_id.as_str(),
            Some(terminal.id.as_str()),
            terminal.project_id.as_deref(),
            "created",
            Some(&terminal),
        );
        publish_terminal_state_changed(user_id.as_str(), &terminal, false, "created", None);
        if let Err(err) = create_local_terminal_session(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            terminal.id.as_str(),
            root_ref.relative_path.as_deref(),
            120,
            32,
        )
        .await
        {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": connector_error_message(err) })),
            );
        }
        (terminal, false)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "error": "未找到空闲 Local Connector 终端，且未允许自动创建" }),
            ),
        );
    };

    let input = format!("{command}\n");
    if let Err(err) = send_local_terminal_input(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        terminal.id.as_str(),
        input.as_str(),
    )
    .await
    {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": connector_error_message(err) })),
        );
    }

    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "command".to_string(),
        command.clone(),
    ))
    .await;
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "input".to_string(),
        input,
    ))
    .await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "terminal_id": terminal.id,
            "terminal_name": terminal.name,
            "terminal_reused": reused,
            "cwd": display_path(terminal.cwd.as_str()),
            "display_cwd": display_path(terminal.cwd.as_str()),
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
    let root_ref = match require_local_connector_root(terminal.cwd.as_str()).await {
        Ok((root_ref, _)) => root_ref,
        Err(err) => return err,
    };
    let reason = normalize_non_empty(req.reason).unwrap_or_else(|| "manual_interrupt".to_string());
    let result = if reason == "project_run_restart" {
        close_local_terminal_session(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            terminal.id.as_str(),
        )
        .await
    } else {
        send_local_terminal_input(
            root_ref.device_id.as_str(),
            root_ref.workspace_id.as_str(),
            terminal.id.as_str(),
            "\u{3}",
        )
        .await
    };
    if let Err(err) = result {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": connector_error_message(err) })),
        );
    }
    let signal = if reason == "project_run_restart" {
        "close"
    } else {
        "SIGINT"
    };
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "signal".to_string(),
        format!("{signal}:{reason}"),
    ))
    .await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "terminal_id": terminal.id,
            "terminal_name": terminal.name,
            "interrupted": true,
            "signal": signal,
            "reason": reason,
        })),
    )
}
