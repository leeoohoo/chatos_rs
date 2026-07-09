// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{http::StatusCode, Json};
use serde_json::Value;
use std::collections::HashMap;

use crate::api::local_connectors::{
    create_local_terminal_session, parse_local_connector_root_path, send_local_terminal_input,
    LocalConnectorRootRef,
};
use crate::models::terminal::{Terminal, TerminalService, TERMINAL_KIND_PROJECT_RUN};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::terminals;
use crate::services::project_run::RunDispatchResult;
use crate::services::realtime::{
    publish_project_run_instance_changed, publish_project_run_state_changed,
};

fn local_connector_refs_match(
    project_root: &LocalConnectorRootRef,
    cwd: &LocalConnectorRootRef,
) -> bool {
    if project_root.device_id != cwd.device_id || project_root.workspace_id != cwd.workspace_id {
        return false;
    }
    let project_relative = project_root.relative_path.as_deref().unwrap_or("");
    let cwd_relative = cwd.relative_path.as_deref().unwrap_or("");
    project_relative.is_empty()
        || cwd_relative == project_relative
        || cwd_relative.starts_with(format!("{project_relative}/").as_str())
}

fn shell_quote_local_value(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn build_local_connector_project_run_input(
    command: &str,
    env_overrides: &HashMap<String, String>,
) -> String {
    let mut entries = env_overrides.iter().collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut input = String::new();
    for (key, value) in entries {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        input.push_str(
            format!("export {key}={}\n", shell_quote_local_value(value.as_str())).as_str(),
        );
    }
    input.push_str(command.trim());
    input.push('\n');
    input
}

fn connector_error_response_message(err: (StatusCode, Json<Value>)) -> String {
    let (status, Json(value)) = err;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(|message| format!("{message} ({status})"))
        .unwrap_or_else(|| format!("{value} ({status})"))
}

pub(super) async fn dispatch_local_connector_project_run(
    user_id: &str,
    project_id: &str,
    project_name: &str,
    project_root: &str,
    cwd: &str,
    command: &str,
    create_if_missing: bool,
    env_overrides: HashMap<String, String>,
    preferred_terminal_id: Option<&str>,
) -> Result<RunDispatchResult, String> {
    let project_ref = parse_local_connector_root_path(project_root)
        .ok_or_else(|| "Local Connector 项目根目录格式错误".to_string())?;
    let cwd_ref = parse_local_connector_root_path(cwd)
        .ok_or_else(|| "Local Connector 运行目录格式错误".to_string())?;
    if !local_connector_refs_match(&project_ref, &cwd_ref) {
        return Err("Local Connector 运行目录必须位于项目目录内".to_string());
    }
    if command.trim().is_empty() {
        return Err("运行命令不能为空".to_string());
    }

    let reusable = if let Some(terminal_id) = preferred_terminal_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let terminal = TerminalService::get_by_id(terminal_id).await?;
        terminal.filter(|item| {
            item.kind == TERMINAL_KIND_PROJECT_RUN
                && item.user_id.as_deref() == Some(user_id)
                && item.project_id.as_deref() == Some(project_id)
                && item.status == "running"
        })
    } else {
        None
    };

    let (terminal, reused) = if let Some(terminal) = reusable {
        (terminal, true)
    } else if create_if_missing {
        let terminal_name = if project_name.trim().is_empty() {
            "Local Connector 运行实例".to_string()
        } else {
            format!("{} 运行实例", project_name.trim())
        };
        let terminal = Terminal::new(
            terminal_name,
            cwd.trim().to_string(),
            TERMINAL_KIND_PROJECT_RUN.to_string(),
            Some(user_id.to_string()),
            Some(project_id.to_string()),
        );
        terminals::create_terminal(&terminal).await?;
        publish_project_run_instance_changed(
            user_id, project_id, &terminal, false, true, "running", "created", None,
        );
        publish_project_run_state_changed(
            user_id,
            project_id,
            Some(&terminal),
            false,
            true,
            "running",
            "created",
            None,
        );
        (terminal, false)
    } else {
        return Err("未找到可复用终端，且未允许自动创建".to_string());
    };

    create_local_terminal_session(
        cwd_ref.device_id.as_str(),
        cwd_ref.workspace_id.as_str(),
        terminal.id.as_str(),
        cwd_ref.relative_path.as_deref(),
        120,
        32,
    )
    .await
    .map_err(connector_error_response_message)?;

    let input = build_local_connector_project_run_input(command, &env_overrides);
    send_local_terminal_input(
        cwd_ref.device_id.as_str(),
        cwd_ref.workspace_id.as_str(),
        terminal.id.as_str(),
        input.as_str(),
    )
    .await
    .map_err(connector_error_response_message)?;

    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "command".to_string(),
        command.trim().to_string(),
    ))
    .await;
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "input".to_string(),
        input,
    ))
    .await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;
    publish_project_run_instance_changed(
        user_id,
        project_id,
        &terminal,
        true,
        true,
        "running",
        "command_dispatched",
        None,
    );
    publish_project_run_state_changed(
        user_id,
        project_id,
        Some(&terminal),
        true,
        true,
        "running",
        "command_dispatched",
        None,
    );

    Ok(RunDispatchResult {
        terminal_id: terminal.id,
        terminal_name: terminal.name,
        terminal_reused: reused,
        terminal_status: terminal.status,
        cwd: cwd.trim().to_string(),
        executed_command: command.trim().to_string(),
    })
}
