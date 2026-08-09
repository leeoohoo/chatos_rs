// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::workspace::directory_ops::create_workspace_directory;
use crate::workspace::paths::workspace_for_request;
use crate::LocalState;

#[derive(Debug, Deserialize)]
struct WorkspaceDirectoryCreateRequest {
    path: Option<String>,
}

pub(crate) async fn handle_workspace_directory_create_request(
    value: Value,
    state: &LocalState,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response(
                "workspace_directory_create_response",
                "",
                400,
                err.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<WorkspaceDirectoryCreateRequest>(request.body.clone())
    {
        Ok(body) => body,
        Err(err) => {
            return workspace_directory_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let workspace = match workspace_for_request(state, request.workspace_id.as_str()) {
        Ok(workspace) => workspace,
        Err(err) => {
            return workspace_directory_create_response(
                request.request_id,
                400,
                json!({ "error": err.to_string() }),
            );
        }
    };
    let path = match body.path {
        Some(path) => path,
        None => {
            return workspace_directory_create_response(
                request.request_id,
                400,
                json!({ "error": "missing field `path`" }),
            );
        }
    };
    match create_workspace_directory(workspace, path.as_str()) {
        Ok(path) => workspace_directory_create_response(
            request.request_id,
            200,
            json!({
                "path": path,
                "created": true,
            }),
        ),
        Err(err) => workspace_directory_create_response(
            request.request_id,
            400,
            json!({ "error": err.to_string() }),
        ),
    }
}

fn workspace_directory_create_response(request_id: String, status: u16, body: Value) -> Value {
    RelayResponse {
        message_type: "workspace_directory_create_response".to_string(),
        request_id,
        status,
        headers: BTreeMap::new(),
        body,
    }
    .into_value()
}
