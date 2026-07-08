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
    let app = build_app(config.clone())?;
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

fn build_app(config: ServerConfig) -> Result<Router, String> {
    std::fs::create_dir_all(&config.workspace).map_err(|err| err.to_string())?;
    std::fs::create_dir_all(&config.state_dir).map_err(|err| err.to_string())?;
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

    Ok(Router::new()
        .route("/health", get(health))
        .route("/mcp", post(mcp_entrypoint))
        .with_state(state))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use chatos_mcp_service::{MCP_ERROR_AUTH_REQUIRED, METHOD_TOOLS_LIST};
    use serde_json::json;
    use tower::ServiceExt;

    fn test_config(name: &str, auth_token: Option<&str>) -> ServerConfig {
        let root = std::env::temp_dir().join(format!(
            "chatos-sandbox-mcp-server-test-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            workspace: root.join("workspace"),
            state_dir: root.join("state"),
            auth_token: auth_token.map(ToOwned::to_owned),
            project_id: Some("project-1".to_string()),
            user_id: Some("user-1".to_string()),
            max_file_bytes: 1024 * 1024,
            max_write_bytes: 1024 * 1024,
            search_limit: 50,
            terminal_idle_timeout_ms: 1_000,
            terminal_max_wait_ms: 1_000,
            terminal_max_output_chars: 4_000,
        }
    }

    fn test_app(name: &str, auth_token: Option<&str>) -> (Router, ServerConfig) {
        let config = test_config(name, auth_token);
        let app = build_app(config.clone()).expect("build app");
        (app, config)
    }

    async fn post_mcp(
        app: Router,
        body: serde_json::Value,
        headers: &[(&str, &str)],
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::CONTENT_TYPE, "application/json");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let response = app
            .oneshot(
                builder
                    .body(Body::from(body.to_string()))
                    .expect("build request"),
            )
            .await
            .expect("handle request");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        let rpc = serde_json::from_slice::<serde_json::Value>(&bytes).expect("decode JSON-RPC");
        (status, rpc)
    }

    fn rpc_request(id: &str, method: &str, params: serde_json::Value) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
    }

    #[tokio::test]
    async fn mcp_entrypoint_returns_jsonrpc_auth_error_for_missing_token() {
        let (app, _config) = test_app("missing-token", Some("secret"));
        let (status, rpc) = post_mcp(
            app,
            rpc_request("auth-1", METHOD_TOOLS_LIST, json!({})),
            &[],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            rpc.get("jsonrpc").and_then(serde_json::Value::as_str),
            Some("2.0")
        );
        assert_eq!(rpc.get("id"), Some(&json!("auth-1")));
        assert_eq!(
            rpc.pointer("/error/code")
                .and_then(serde_json::Value::as_i64),
            Some(i64::from(MCP_ERROR_AUTH_REQUIRED))
        );
        assert!(rpc.get("result").is_none());
    }

    #[tokio::test]
    async fn mcp_entrypoint_returns_jsonrpc_auth_error_for_wrong_bearer_token() {
        let (app, _config) = test_app("wrong-token", Some("secret"));
        let (status, rpc) = post_mcp(
            app,
            rpc_request("auth-2", METHOD_TOOLS_LIST, json!({})),
            &[(header::AUTHORIZATION.as_str(), "Bearer wrong")],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(rpc.get("id"), Some(&json!("auth-2")));
        assert_eq!(
            rpc.pointer("/error/code")
                .and_then(serde_json::Value::as_i64),
            Some(i64::from(MCP_ERROR_AUTH_REQUIRED))
        );
    }

    #[tokio::test]
    async fn mcp_entrypoint_handles_jsonrpc_methods_with_bearer_token() {
        let (app, config) = test_app("bearer-success", Some("secret"));
        std::fs::write(config.workspace.join("hello.txt"), "hello from sandbox")
            .expect("write fixture");
        let auth = [(header::AUTHORIZATION.as_str(), "Bearer secret")];

        let (_status, initialize) = post_mcp(
            app.clone(),
            rpc_request("init-1", "initialize", json!({})),
            &auth,
        )
        .await;
        assert_eq!(
            initialize
                .pointer("/result/serverInfo/name")
                .and_then(serde_json::Value::as_str),
            Some("chatos-sandbox-mcp-server")
        );

        let (_status, ping) =
            post_mcp(app.clone(), rpc_request("ping-1", "ping", json!({})), &auth).await;
        assert_eq!(ping.get("result"), Some(&json!({})));

        let (_status, tools) = post_mcp(
            app.clone(),
            rpc_request("tools-1", METHOD_TOOLS_LIST, json!({})),
            &auth,
        )
        .await;
        let tool_names: Vec<&str> = tools
            .pointer("/result/tools")
            .and_then(serde_json::Value::as_array)
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
            .collect();
        assert!(tool_names.contains(&"read_file"));

        let (_status, call) = post_mcp(
            app,
            rpc_request(
                "call-1",
                "tools/call",
                json!({
                    "name": "read_file",
                    "arguments": { "path": "hello.txt" },
                }),
            ),
            &auth,
        )
        .await;
        assert_eq!(call.get("id"), Some(&json!("call-1")));
        assert!(
            call.get("error").is_none(),
            "unexpected error: {:?}",
            call.get("error")
        );
        assert!(
            call.get("result")
                .map(|value| value.to_string().contains("hello from sandbox"))
                .unwrap_or(false),
            "tools/call result should include fixture content"
        );
    }

    #[tokio::test]
    async fn mcp_entrypoint_accepts_sandbox_token_header() {
        let (app, _config) = test_app("sandbox-header", Some("secret"));
        let (_status, rpc) = post_mcp(
            app,
            rpc_request("tools-2", METHOD_TOOLS_LIST, json!({})),
            &[("x-chatos-sandbox-token", "secret")],
        )
        .await;

        assert!(rpc.get("error").is_none());
        assert!(rpc
            .pointer("/result/tools")
            .and_then(serde_json::Value::as_array)
            .is_some());
    }
}
