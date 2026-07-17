// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::project_execution::require_local_connector_desktop;
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::normalize_non_empty;

use super::connector_client::connector_post_json;
use super::types::{
    LocalTerminalExecRequest, RelayTerminalExecRequest, RelayTerminalInputRequest,
    RelayTerminalSessionCreateRequest,
};
use super::{load_owned_online_workspace, required_text};

pub(super) async fn exec_terminal_command(
    auth: AuthUser,
    headers: HeaderMap,
    Json(req): Json<LocalTerminalExecRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = require_local_connector_desktop(&headers) {
        return err;
    }
    if let Err(err) = resolve_user_id(req.user_id, &auth) {
        return err;
    }
    let device_id = match required_text(req.device_id, "device_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    let workspace_id = match required_text(req.workspace_id, "workspace_id") {
        Ok(value) => value,
        Err(err) => return err,
    };
    let command = match required_text(req.command, "command") {
        Ok(value) => value,
        Err(err) => return err,
    };
    if let Err(err) = load_owned_online_workspace(device_id.as_str(), workspace_id.as_str()).await {
        return err;
    }
    let path = format!(
        "/api/local-connectors/relay/{}/terminal/exec",
        urlencoding::encode(device_id.as_str())
    );
    match connector_post_json::<Value, _>(
        path.as_str(),
        &RelayTerminalExecRequest {
            workspace_id: workspace_id.as_str(),
            command: command.as_str(),
            args: req.args.unwrap_or_default(),
            cwd: normalize_non_empty(req.cwd),
            timeout_ms: req.timeout_ms,
            source: normalize_non_empty(req.source),
        },
    )
    .await
    {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => err,
    }
}

pub(crate) async fn create_local_terminal_session(
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    cwd: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/local-connectors/relay/{}/terminal/sessions",
        urlencoding::encode(device_id)
    );
    connector_post_json::<Value, _>(
        path.as_str(),
        &RelayTerminalSessionCreateRequest {
            workspace_id,
            terminal_session_id,
            cwd,
            cols: cols.max(1),
            rows: rows.max(1),
        },
    )
    .await
}

pub(crate) async fn send_local_terminal_input(
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    data: &str,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let path = format!(
        "/api/local-connectors/relay/{}/terminal/input",
        urlencoding::encode(device_id)
    );
    connector_post_json::<Value, _>(
        path.as_str(),
        &RelayTerminalInputRequest {
            workspace_id,
            terminal_session_id,
            data,
        },
    )
    .await
}
