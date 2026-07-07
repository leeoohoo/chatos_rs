// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use chatos_mcp_service::{JsonRpcRequest, McpJsonRpcService, McpServerInfo};
use serde_json::{json, Value};

use crate::history::CommandHistoryRecorder;
use crate::mcp::terminal::{handle_local_mcp_terminal_cleanup, handle_local_mcp_terminal_start};
use crate::relay::{relay_error_response, RelayRequest, RelayResponse, MCP_RELAY_MESSAGE_TYPE};
use crate::LocalState;

use super::provider::LocalConnectorMcpToolProvider;

pub(crate) async fn handle_mcp_request(
    value: Value,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response(MCP_RELAY_MESSAGE_TYPE, "", 400, err.to_string());
        }
    };
    let body = match handle_mcp_body(&request, state, history_recorder).await {
        Ok(body) => body,
        Err(err) => {
            return RelayResponse {
                message_type: MCP_RELAY_MESSAGE_TYPE.to_string(),
                request_id: request.request_id,
                status: 400,
                headers: BTreeMap::new(),
                body: json!({ "error": err.to_string() }),
            }
            .to_value();
        }
    };
    RelayResponse {
        message_type: MCP_RELAY_MESSAGE_TYPE.to_string(),
        request_id: request.request_id,
        status: 200,
        headers: BTreeMap::new(),
        body,
    }
    .to_value()
}

pub(crate) async fn handle_mcp_body(
    request: &RelayRequest,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let body = &request.body;
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match method {
        "local_connector/terminal/start" => {
            handle_local_mcp_terminal_start(request, state, history_recorder).await
        }
        "local_connector/terminal/cleanup" => {
            handle_local_mcp_terminal_cleanup(request, state).await
        }
        _ => handle_standard_local_mcp_body(request, state, history_recorder).await,
    }
}

async fn handle_standard_local_mcp_body(
    request: &RelayRequest,
    state: &LocalState,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let rpc_request = serde_json::from_value::<JsonRpcRequest>(request.body.clone())
        .context("parse local connector MCP JSON-RPC request")?;
    let provider = LocalConnectorMcpToolProvider {
        request: request.clone(),
        state: state.clone(),
        history_recorder: history_recorder.clone(),
    };
    let service = McpJsonRpcService::new(
        McpServerInfo::new("local_connector", env!("CARGO_PKG_VERSION")),
        Arc::new(provider),
    );
    serde_json::to_value(service.handle(rpc_request).await)
        .context("serialize local connector MCP JSON-RPC response")
}
