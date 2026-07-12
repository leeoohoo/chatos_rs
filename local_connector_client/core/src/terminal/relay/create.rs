// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::terminal::session::LocalTerminalManager;
use crate::workspace::paths::{
    canonicalize_existing_dir, resolve_request_workspace_dir, workspace_for_request,
};
use crate::LocalState;

use super::types::TerminalSessionCreateRequest;

pub(crate) async fn handle_terminal_session_create_request(
    value: Value,
    state: &LocalState,
    terminal_manager: &LocalTerminalManager,
    outbound_tx: mpsc::UnboundedSender<Value>,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response(
                "terminal_session_create_response",
                "",
                400,
                err.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<TerminalSessionCreateRequest>(request.body.clone()) {
        Ok(body) => body,
        Err(err) => {
            return terminal_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let workspace = match workspace_for_request(state, request.workspace_id.as_str()) {
        Ok(workspace) => workspace,
        Err(err) => {
            return terminal_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let cwd = match resolve_request_workspace_dir(
        workspace,
        &request,
        body.cwd.as_deref().unwrap_or("."),
    ) {
        Ok(cwd) => cwd,
        Err(err) => {
            return terminal_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let root_cwd = match canonicalize_existing_dir(workspace.absolute_root.as_path()) {
        Ok(root) => root,
        Err(err) => {
            return terminal_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let session = match terminal_manager
        .ensure_session(
            body.terminal_session_id.clone(),
            root_cwd,
            cwd,
            body.cols.unwrap_or(80).max(1),
            body.rows.unwrap_or(24).max(1),
            outbound_tx,
        )
        .await
    {
        Ok(session) => session,
        Err(err) => {
            return terminal_create_response(
                request.request_id,
                500,
                json!({ "error": err.to_string() }),
            );
        }
    };
    terminal_create_response(
        request.request_id,
        200,
        json!({
            "terminal_session_id": body.terminal_session_id,
            "snapshot": session.snapshot(500),
            "busy": session.busy(),
        }),
    )
}

fn terminal_create_response(request_id: String, status: u16, body: Value) -> Value {
    RelayResponse {
        message_type: "terminal_session_create_response".to_string(),
        request_id,
        status,
        headers: BTreeMap::new(),
        body,
    }
    .into_value()
}
