// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod agent_relay;
mod auth;
mod command_sandbox;
mod config;
mod network_proxy;
mod network_proxy_mitm;
mod quota;
mod terminal_store;
mod tools;

use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
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
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::quota::WorkspaceQuota;
use crate::terminal_store::SandboxTerminalControllerStore;
use crate::tools::SandboxMcpToolProvider;
use crate::{auth::authorize, config::ServerConfig};

const VERSION: &str = "0.1.0";

#[derive(Clone)]
struct AppState {
    config: ServerConfig,
    workspace_writes_allowed: bool,
    started_at: String,
    tools: Vec<Value>,
    mcp_service: McpJsonRpcService,
}

#[derive(Debug)]
struct NoopCodeMaintainerHooks;

impl CodeMaintainerHooks for NoopCodeMaintainerHooks {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The dependency graph can enable both rustls crypto backends. Select one deterministically
    // before any proxy or transitive HTTP client builds a TLS configuration.
    let _ = rustls::crypto::ring::default_provider().install_default();
    if agent_relay::is_internal_agent_relay() {
        agent_relay::run_internal_agent_relay()
            .await
            .map_err(std::io::Error::other)?;
        return Ok(());
    }
    if network_proxy::is_internal_command_wrapper() {
        network_proxy::run_internal_command_wrapper()
            .await
            .map_err(std::io::Error::other)?;
        return Ok(());
    }
    if network_proxy::is_internal_network_proxy_wrapper() {
        network_proxy::run_internal_network_proxy_wrapper()
            .await
            .map_err(std::io::Error::other)?;
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,chatos_sandbox_mcp_server=debug".into()),
        )
        .init();

    let config = ServerConfig::from_env()?;
    if std::env::var("CHATOS_SANDBOX_TRANSPORT")
        .is_ok_and(|value| value.trim().eq_ignore_ascii_case("stdio"))
    {
        let state = build_state(config).await.map_err(std::io::Error::other)?;
        run_stdio(state).await?;
        return Ok(());
    }
    let app = build_app(config.clone()).await?;
    #[cfg(unix)]
    if let Some(socket_path) = configured_agent_unix_socket()? {
        return serve_agent_unix_socket(app, socket_path.as_path()).await;
    }
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

#[cfg(unix)]
fn configured_agent_unix_socket() -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let Some(value) = std::env::var_os("CHATOS_AGENT_UNIX_SOCKET") else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err("CHATOS_AGENT_UNIX_SOCKET must be an absolute path".into());
    }
    Ok(Some(path))
}

#[cfg(unix)]
async fn serve_agent_unix_socket(
    app: Router,
    socket_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let parent = socket_path
        .parent()
        .ok_or("CHATOS_AGENT_UNIX_SOCKET has no parent directory")?;
    let parent_metadata = std::fs::symlink_metadata(parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err("sandbox agent socket directory must be a non-symlink directory".into());
    }
    match std::fs::symlink_metadata(socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(socket_path)?;
        }
        Ok(_) => return Err("sandbox agent socket path is occupied by a non-socket file".into()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    let listener = UnixListener::bind(socket_path)?;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    info!(
        service = "chatos-sandbox-mcp-server",
        version = VERSION,
        socket = %socket_path.display(),
        "sandbox MCP server started on Unix socket"
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn build_app(config: ServerConfig) -> Result<Router, String> {
    let state = build_state(config).await?;
    Ok(Router::new()
        .route("/health", get(health))
        .route("/mcp", post(mcp_entrypoint))
        .with_state(state))
}

async fn build_state(config: ServerConfig) -> Result<AppState, String> {
    std::fs::create_dir_all(&config.workspace).map_err(|err| err.to_string())?;
    std::fs::create_dir_all(&config.state_dir).map_err(|err| err.to_string())?;
    let command_sandbox =
        crate::command_sandbox::CommandSandboxConfig::from_server_config(&config).await?;
    let file_access_policy = Arc::new(command_sandbox.file_tool_access_policy()?);
    let workspace_writes_allowed = file_access_policy.workspace_writes_allowed();
    let file_service = build_file_service(&config, workspace_writes_allowed)?;
    let terminal_service = build_terminal_service(&config, command_sandbox)?;
    let workspace_quota = WorkspaceQuota::new(config.workspace.clone(), config.disk_limit_bytes)
        .with_extra_roots(config.extra_quota_roots.clone());
    let provider = SandboxMcpToolProvider::new(
        file_service,
        terminal_service,
        workspace_quota,
        file_access_policy,
    );
    let tools = provider.tools();
    let mcp_service = McpJsonRpcService::new(
        McpServerInfo::new("chatos-sandbox-mcp-server", VERSION),
        Arc::new(provider),
    );

    Ok(AppState {
        config: config.clone(),
        workspace_writes_allowed,
        started_at: chrono::Utc::now().to_rfc3339(),
        tools,
        mcp_service,
    })
}

fn build_file_service(
    config: &ServerConfig,
    allow_writes: bool,
) -> Result<CodeMaintainerService, String> {
    let change_log_path = config.state_dir.join("code-maintainer.changes.jsonl");
    CodeMaintainerService::new(CodeMaintainerOptions {
        server_name: "sandbox_code_maintainer".to_string(),
        root: config.workspace.clone(),
        project_id: config.project_id.clone(),
        allow_writes,
        max_file_bytes: config.max_file_bytes,
        max_write_bytes: config.max_write_bytes,
        search_limit: config.search_limit,
        enable_read_tools: true,
        enable_write_tools: allow_writes,
        conversation_id: None,
        run_id: None,
        db_path: Some(change_log_path.to_string_lossy().to_string()),
        hooks: Some(CodeMaintainerHooksRef::new(Arc::new(
            NoopCodeMaintainerHooks,
        ))),
    })
}

fn build_terminal_service(
    config: &ServerConfig,
    command_sandbox: crate::command_sandbox::CommandSandboxConfig,
) -> Result<TerminalControllerService, String> {
    TerminalControllerService::new(TerminalControllerOptions {
        root: config.workspace.clone(),
        user_id: config.user_id.clone(),
        project_id: config.project_id.clone(),
        idle_timeout_ms: config.terminal_idle_timeout_ms,
        max_wait_ms: config.terminal_max_wait_ms,
        max_output_chars: config.terminal_max_output_chars,
        store: TerminalControllerStoreRef::new(Arc::new(SandboxTerminalControllerStore::new(
            WorkspaceQuota::new(config.workspace.clone(), config.disk_limit_bytes)
                .with_extra_roots(config.extra_quota_roots.clone()),
            command_sandbox,
        ))),
    })
}

async fn run_stdio(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let writes_required = state.workspace_writes_allowed;
    if !probe_workspace_access(state.config.workspace.as_path(), writes_required) {
        return Err(std::io::Error::other("sandbox workspace is not accessible").into());
    }
    WorkspaceQuota::new(
        state.config.workspace.clone(),
        state.config.disk_limit_bytes,
    )
    .with_extra_roots(state.config.extra_quota_roots.clone())
    .check_sync()
    .map_err(std::io::Error::other)?;

    info!(
        service = "chatos-sandbox-mcp-server",
        version = VERSION,
        transport = "stdio",
        workspace = %state.config.workspace.display(),
        "sandbox MCP server started"
    );
    let (input_tx, mut input_rx) = mpsc::channel::<Result<String, std::io::Error>>(16);
    tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if input_tx.send(Ok(line)).await.is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    terminate_owned_process_group();
                    return;
                }
                Err(err) => {
                    let _ = input_tx.send(Err(err)).await;
                    terminate_owned_process_group();
                    return;
                }
            }
        }
    });
    let mut stdout = BufWriter::new(tokio::io::stdout());
    while let Some(line) = input_rx.recv().await {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => state.mcp_service.handle(request).await,
            Err(err) => jsonrpc_error(
                Value::Null,
                -32700,
                format!("invalid JSON-RPC request: {err}"),
            ),
        };
        let mut encoded = serde_json::to_vec(&response)?;
        encoded.push(b'\n');
        stdout.write_all(&encoded).await?;
        stdout.flush().await?;
    }
    Ok(())
}

#[cfg(unix)]
fn terminate_owned_process_group() {
    if std::env::var("CHATOS_SANDBOX_PROCESS_GROUP_OWNED").as_deref() != Ok("1") {
        return;
    }
    unsafe {
        libc::kill(0, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn terminate_owned_process_group() {}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let writes_required = state.workspace_writes_allowed;
    let workspace_accessible =
        probe_workspace_access(state.config.workspace.as_path(), writes_required);
    let quota = WorkspaceQuota::new(
        state.config.workspace.clone(),
        state.config.disk_limit_bytes,
    )
    .with_extra_roots(state.config.extra_quota_roots.clone());
    let quota_result = quota.check_sync();
    let ok = workspace_accessible && quota_result.is_ok();
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "ok": ok,
            "service": "chatos-sandbox-mcp-server",
            "agent_version": VERSION,
            "workspace": state.config.workspace,
            "workspace_writable": writes_required && workspace_accessible,
            "workspace_writes_allowed": writes_required,
            "workspace_disk_limit_bytes": state.config.disk_limit_bytes,
            "workspace_disk_used_bytes": quota_result.as_ref().ok(),
            "workspace_disk_error": quota_result.err(),
            "mcp_endpoint": "/mcp",
            "tools_count": state.tools.len(),
            "started_at": state.started_at,
        })),
    )
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

fn probe_workspace_access(workspace: &Path, writes_required: bool) -> bool {
    if let Err(err) = std::fs::create_dir_all(workspace) {
        warn!(
            error = err.to_string(),
            "create workspace for health probe failed"
        );
        return false;
    }
    if let Err(err) = std::fs::read_dir(workspace) {
        warn!(error = err.to_string(), "workspace read probe failed");
        return false;
    }
    if !writes_required {
        return true;
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
    use chatos_sandbox_contract::{
        legacy_policy_permission_snapshot, EffectiveSandboxPolicy, FileSystemAccessMode,
        FileSystemPath, FileSystemPermissionPolicy, FileSystemSandboxEntry, PermissionProfileId,
    };
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
            disk_limit_bytes: Some(8 * 1024 * 1024),
            extra_quota_roots: Vec::new(),
            permission_profile: "workspace_write".to_string(),
            command_sandbox_backend: "external".to_string(),
            additional_writable_roots: Vec::new(),
            host_home: None,
            effective_permissions: None,
        }
    }

    async fn test_app(name: &str, auth_token: Option<&str>) -> (Router, ServerConfig) {
        let config = test_config(name, auth_token);
        let app = build_app(config.clone()).await.expect("build app");
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
        let (app, _config) = test_app("missing-token", Some("secret")).await;
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
        let (app, _config) = test_app("wrong-token", Some("secret")).await;
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
        let (app, config) = test_app("bearer-success", Some("secret")).await;
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
    async fn file_tools_enforce_effective_permission_snapshot() {
        let mut config = test_config("file-policy", None);
        std::fs::create_dir_all(config.workspace.join(".git")).expect("git directory");
        let secret = config.workspace.join("secret.env");
        std::fs::write(secret.as_path(), "secret").expect("secret fixture");
        let policy = EffectiveSandboxPolicy {
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            ..EffectiveSandboxPolicy::default()
        };
        let mut snapshot = legacy_policy_permission_snapshot(
            &policy,
            vec![config.workspace.to_string_lossy().to_string()],
        );
        let FileSystemPermissionPolicy::Restricted { entries, .. } = &mut snapshot.file_system
        else {
            panic!("workspace profile must be restricted");
        };
        entries.push(FileSystemSandboxEntry {
            access: FileSystemAccessMode::Deny,
            path: FileSystemPath::Path {
                path: secret.to_string_lossy().to_string(),
            },
        });
        config.effective_permissions = Some(snapshot);
        let app = build_app(config.clone()).await.expect("build app");

        let (_status, ordinary) = post_mcp(
            app.clone(),
            rpc_request(
                "write-ordinary",
                "tools/call",
                json!({
                    "name": "write_file",
                    "arguments": { "path": "ordinary.txt", "content": "ok" },
                }),
            ),
            &[],
        )
        .await;
        assert!(
            !ordinary.to_string().contains("permission profile denies"),
            "ordinary workspace write should remain allowed: {ordinary}"
        );
        assert!(config.workspace.join("ordinary.txt").exists());

        let (_status, protected_write) = post_mcp(
            app.clone(),
            rpc_request(
                "write-protected",
                "tools/call",
                json!({
                    "name": "write_file",
                    "arguments": { "path": ".git/config", "content": "blocked" },
                }),
            ),
            &[],
        )
        .await;
        assert!(
            protected_write
                .to_string()
                .contains("permission profile denies writing"),
            "metadata write must be denied: {protected_write}"
        );

        let (_status, denied_read) = post_mcp(
            app,
            rpc_request(
                "read-denied",
                "tools/call",
                json!({
                    "name": "read_file",
                    "arguments": { "path": "secret.env" },
                }),
            ),
            &[],
        )
        .await;
        assert!(
            denied_read
                .to_string()
                .contains("permission profile denies reading"),
            "denied file contents must not be returned: {denied_read}"
        );
        let _ = std::fs::remove_dir_all(config.workspace.parent().expect("test workspace parent"));
    }

    #[tokio::test]
    async fn mcp_entrypoint_accepts_sandbox_token_header() {
        let (app, _config) = test_app("sandbox-header", Some("secret")).await;
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
