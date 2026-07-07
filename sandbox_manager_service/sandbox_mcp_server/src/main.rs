// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod auth;
mod config;
mod terminal_store;
mod tools;

use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_builtin_tools::{
    CodeMaintainerHooks, CodeMaintainerHooksRef, CodeMaintainerOptions, CodeMaintainerService,
    TerminalControllerOptions, TerminalControllerService, TerminalControllerStoreRef,
};
use chatos_mcp_service::{
    jsonrpc_error, JsonRpcRequest, JsonRpcResponse, McpJsonRpcService, McpServerInfo,
    MCP_ERROR_AUTH_REQUIRED,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::terminal_store::SandboxTerminalControllerStore;
use crate::tools::SandboxMcpToolProvider;
use crate::{auth::authorize, config::ServerConfig};

const VERSION: &str = "0.1.0";

#[derive(Clone)]
struct AppState {
    config: ServerConfig,
    started_at: String,
    tools: Vec<Value>,
    mcp_service: McpJsonRpcService,
}

#[derive(Debug)]
struct NoopCodeMaintainerHooks;

impl CodeMaintainerHooks for NoopCodeMaintainerHooks {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,chatos_sandbox_mcp_server=debug".into()),
        )
        .init();

    let config = ServerConfig::from_env()?;
    std::fs::create_dir_all(&config.workspace)?;
    std::fs::create_dir_all(&config.state_dir)?;

    let file_service = build_file_service(&config)?;
    let terminal_service = build_terminal_service(&config)?;
    let provider = SandboxMcpToolProvider::new(file_service, terminal_service);
    let tools = provider.tools();
    let mcp_service = McpJsonRpcService::new(
        McpServerInfo::new("chatos-sandbox-mcp-server", VERSION),
        Arc::new(provider),
    );

    let state = AppState {
        config: config.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        tools,
        mcp_service,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/mcp", post(mcp_entrypoint))
        .with_state(state);

    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(bind_addr.as_str()).await?;
    info!(
        service = "chatos-sandbox-mcp-server",
        version = VERSION,
        bind_addr = bind_addr.as_str(),
        workspace = %config.workspace.display(),
        "sandbox MCP server started"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_file_service(config: &ServerConfig) -> Result<CodeMaintainerService, String> {
    let change_log_path = config.state_dir.join("code-maintainer.changes.jsonl");
    CodeMaintainerService::new(CodeMaintainerOptions {
        server_name: "sandbox_code_maintainer".to_string(),
        root: config.workspace.clone(),
        project_id: config.project_id.clone(),
        allow_writes: true,
        max_file_bytes: config.max_file_bytes,
        max_write_bytes: config.max_write_bytes,
        search_limit: config.search_limit,
        enable_read_tools: true,
        enable_write_tools: true,
        conversation_id: None,
        run_id: None,
        db_path: Some(change_log_path.to_string_lossy().to_string()),
        hooks: Some(CodeMaintainerHooksRef::new(Arc::new(
            NoopCodeMaintainerHooks,
        ))),
    })
}

fn build_terminal_service(config: &ServerConfig) -> Result<TerminalControllerService, String> {
    TerminalControllerService::new(TerminalControllerOptions {
        root: config.workspace.clone(),
        user_id: config.user_id.clone(),
        project_id: config.project_id.clone(),
        idle_timeout_ms: config.terminal_idle_timeout_ms,
        max_wait_ms: config.terminal_max_wait_ms,
        max_output_chars: config.terminal_max_output_chars,
        store: TerminalControllerStoreRef::new(Arc::new(SandboxTerminalControllerStore)),
    })
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let workspace_writable = probe_workspace_writable(state.config.workspace.as_path());
    Json(json!({
        "ok": workspace_writable,
        "service": "chatos-sandbox-mcp-server",
        "agent_version": VERSION,
        "workspace": state.config.workspace,
        "workspace_writable": workspace_writable,
        "mcp_endpoint": "/mcp",
        "tools_count": state.tools.len(),
        "started_at": state.started_at,
    }))
}

async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    if let Err(message) = authorize(state.config.auth_token.as_deref(), &headers) {
        return Json(jsonrpc_error(id, MCP_ERROR_AUTH_REQUIRED, message));
    }
    Json(state.mcp_service.handle(request).await)
}

fn probe_workspace_writable(workspace: &Path) -> bool {
    if let Err(err) = std::fs::create_dir_all(workspace) {
        warn!(
            error = err.to_string(),
            "create workspace for health probe failed"
        );
        return false;
    }
    let probe = workspace.join(".chatos_sandbox_mcp_health");
    match std::fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(err) => {
            warn!(error = err.to_string(), "workspace health probe failed");
            false
        }
    }
}
