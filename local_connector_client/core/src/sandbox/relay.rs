// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use reqwest::Method;
use serde_json::{json, Value};

use crate::history::CommandHistoryRecorder;
use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::sandbox::lease::{
    create_local_sandbox_lease, get_local_sandbox, health_local_sandbox, release_local_sandbox,
};
use crate::sandbox::proxy::proxy_local_sandbox_mcp;
use crate::sandbox::types::LocalSandboxRuntime;
use crate::LocalState;

pub(crate) async fn handle_sandbox_request(
    value: Value,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("sandbox_response", "", 400, err.to_string());
        }
    };
    match handle_local_sandbox_request(
        &request,
        state,
        http_client,
        sandbox_runtime,
        history_recorder,
    )
    .await
    {
        Ok((status, headers, body)) => RelayResponse {
            message_type: "sandbox_response".to_string(),
            request_id: request.request_id,
            status,
            headers,
            body,
        }
        .to_value(),
        Err(err) => RelayResponse {
            message_type: "sandbox_response".to_string(),
            request_id: request.request_id,
            status: 502,
            headers: BTreeMap::new(),
            body: json!({ "error": err.to_string() }),
        }
        .to_value(),
    }
}

async fn handle_local_sandbox_request(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let method = request
        .method
        .as_deref()
        .unwrap_or("POST")
        .parse::<Method>()
        .context("parse sandbox request method")?;
    let path = normalize_sandbox_http_path(request.path.as_deref().unwrap_or("/"));
    if method == Method::POST && path == "/api/sandboxes/leases" {
        return create_local_sandbox_lease(request, state, http_client, sandbox_runtime).await;
    }
    if method == Method::GET && path == "/api/sandboxes" {
        let leases = sandbox_runtime
            .leases
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        return Ok((200, BTreeMap::new(), json!(leases)));
    }
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() >= 3 && parts[0] == "api" && parts[1] == "sandboxes" {
        let sandbox_id = parts[2];
        if method == Method::GET && parts.len() == 3 {
            return get_local_sandbox(sandbox_runtime, sandbox_id).await;
        }
        if method == Method::GET && parts.len() == 4 && parts[3] == "health" {
            return health_local_sandbox(http_client, sandbox_runtime, sandbox_id).await;
        }
        if method == Method::POST && parts.len() == 4 && parts[3] == "release" {
            return release_local_sandbox(request, sandbox_runtime, sandbox_id).await;
        }
        if method == Method::POST && parts.len() == 4 && parts[3] == "mcp" {
            return proxy_local_sandbox_mcp(
                request,
                state,
                http_client,
                sandbox_runtime,
                sandbox_id,
                history_recorder,
            )
            .await;
        }
    }
    Ok((
        404,
        BTreeMap::new(),
        json!({ "error": format!("unsupported local sandbox path: {path}") }),
    ))
}

fn normalize_sandbox_http_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}
