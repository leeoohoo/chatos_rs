// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use chatos_mcp_service::{JsonRpcRequest, McpJsonRpcService, McpServerInfo};
use serde_json::{json, Value};

use crate::history::CommandHistoryRecorder;
use crate::local_runtime::LocalDatabase;
use crate::mcp::terminal::{handle_local_mcp_terminal_cleanup, handle_local_mcp_terminal_start};
use crate::relay::{relay_error_response, RelayRequest, RelayResponse, MCP_RELAY_MESSAGE_TYPE};
use crate::sandbox::types::LocalSandboxRuntime;
use crate::LocalState;

use super::provider::LocalConnectorMcpToolProvider;
use super::user_runtime::{handle_user_mcp_body, is_user_mcp_request};

pub(crate) async fn handle_mcp_request(
    value: Value,
    state: &LocalState,
    database: &LocalDatabase,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response(MCP_RELAY_MESSAGE_TYPE, "", 400, err.to_string());
        }
    };
    let body = match handle_mcp_body_with_database(
        &request,
        state,
        database,
        http_client,
        sandbox_runtime,
        history_recorder,
    )
    .await
    {
        Ok(body) => body,
        Err(err) => {
            return RelayResponse {
                message_type: MCP_RELAY_MESSAGE_TYPE.to_string(),
                request_id: request.request_id,
                status: 400,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .into_value();
        }
    };
    RelayResponse {
        message_type: MCP_RELAY_MESSAGE_TYPE.to_string(),
        request_id: request.request_id,
        status: 200,
        headers: BTreeMap::new(),
        body,
    }
    .into_value()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn handle_mcp_body(
    request: &RelayRequest,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    handle_mcp_body_without_user_runtime(request, state, None, None, history_recorder).await
}

async fn handle_mcp_body_with_database(
    request: &RelayRequest,
    state: &LocalState,
    database: &LocalDatabase,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    if is_user_mcp_request(request) {
        return handle_user_mcp_body(request, database).await;
    }
    handle_mcp_body_without_user_runtime(
        request,
        state,
        Some((http_client, sandbox_runtime)),
        Some(database),
        history_recorder,
    )
    .await
}

async fn handle_mcp_body_without_user_runtime(
    request: &RelayRequest,
    state: &LocalState,
    execution_runtime: Option<(&reqwest::Client, &LocalSandboxRuntime)>,
    database: Option<&LocalDatabase>,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let body = &request.body;
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match method {
        "local_connector/execution_scope/finalize" => {
            let (_, sandbox_runtime) = execution_runtime
                .ok_or_else(|| anyhow::anyhow!("local execution scope runtime is unavailable"))?;
            super::execution_scope::finalize_local_execution_scope(
                request,
                sandbox_runtime,
                database.ok_or_else(|| {
                    anyhow::anyhow!("local execution scope database is unavailable")
                })?,
            )
            .await
        }
        "local_connector/terminal/start" => {
            handle_local_mcp_terminal_start(request, state, history_recorder).await
        }
        "local_connector/terminal/cleanup" => {
            handle_local_mcp_terminal_cleanup(request, state).await
        }
        _ => {
            handle_standard_local_mcp_body(
                request,
                state,
                execution_runtime,
                database,
                history_recorder,
            )
            .await
        }
    }
}

async fn handle_standard_local_mcp_body(
    request: &RelayRequest,
    state: &LocalState,
    execution_runtime: Option<(&reqwest::Client, &LocalSandboxRuntime)>,
    database: Option<&LocalDatabase>,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    if is_user_mcp_request(request) {
        return Err(anyhow::anyhow!(
            "user MCP execution requires the local SQLite runtime"
        ));
    }
    let rpc_request = serde_json::from_value::<JsonRpcRequest>(request.body.clone())
        .context("parse local connector MCP JSON-RPC request")?;
    let provider = LocalConnectorMcpToolProvider {
        request: request.clone(),
        state: state.clone(),
        execution_runtime: execution_runtime
            .map(|(http_client, sandbox_runtime)| (http_client.clone(), sandbox_runtime.clone())),
        database: database.cloned(),
        history_recorder: history_recorder.clone(),
    };
    let service = McpJsonRpcService::new(
        McpServerInfo::new("local_connector", env!("CARGO_PKG_VERSION")),
        Arc::new(provider),
    );
    serde_json::to_value(service.handle(rpc_request).await)
        .context("serialize local connector MCP JSON-RPC response")
}
