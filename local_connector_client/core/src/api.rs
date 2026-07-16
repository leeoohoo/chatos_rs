// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::Result;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as HyperConnectionBuilder;
use hyper_util::service::TowerToHyperService;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::config::{optional_env, DEFAULT_LOCAL_API_PORT};
use crate::{tracing_stdout, LocalRuntime};

mod handlers;
mod types;

use handlers::{
    local_add_workspace, local_agent_prompt_status, local_approval_settings,
    local_approve_pending_approval, local_check_agent_prompt_updates, local_clear_command_history,
    local_command_history, local_delete_mcp_config, local_delete_model_config,
    local_delete_sandbox_image, local_deny_pending_approval, local_desktop_ticket,
    local_disable_mcp_config, local_docker_status, local_enable_mcp_config, local_fs_list_handler,
    local_get_mcp_config, local_initialize_sandbox_image, local_login, local_logout,
    local_mcp_configs, local_model_configs, local_model_settings, local_pending_approvals,
    local_preview_model_catalog, local_register, local_reinitialize_sandbox_image,
    local_remove_workspace, local_request_system_permission, local_runtime_settings,
    local_sandbox_capabilities, local_sandbox_image_jobs, local_sandbox_image_mcp,
    local_sandbox_images, local_sandbox_leases, local_sandbox_settings, local_save_mcp_config,
    local_save_model_config, local_send_register_email_code, local_skills, local_status,
    local_sync_mcp_config, local_sync_model_config, local_sync_skill_inventory,
    local_system_permissions, local_terminal_exec, local_test_mcp_config, local_toggle_sandbox,
    local_update_agent_prompt_bundle, local_update_approval_settings, local_update_mcp_config,
    local_update_model_config, local_update_model_settings, local_update_runtime_settings,
    local_update_sandbox_settings, local_update_skill_preference,
    local_update_workspace_project_config_trust,
};

pub(crate) async fn serve_local_api(runtime: LocalRuntime) -> Result<()> {
    let ipc_endpoint = optional_env("LOCAL_CONNECTOR_IPC_ENDPOINT")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let tcp_enabled = ipc_endpoint.is_none() || env_flag("LOCAL_CONNECTOR_ENABLE_TCP_API");
    let desktop_auth_token = local_desktop_auth_token();
    if tcp_enabled && desktop_auth_token.is_none() {
        return Err(anyhow::anyhow!(
            "LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN is required when the Local Connector TCP API is enabled"
        ));
    }
    let app = local_api_app(runtime, desktop_auth_token);
    if let Some(endpoint) = ipc_endpoint {
        if env_flag("LOCAL_CONNECTOR_ENABLE_TCP_API") {
            let tcp_app = app.clone();
            tokio::spawn(async move {
                if let Err(err) = serve_tcp_local_api(tcp_app).await {
                    tracing_stdout(format!("local connector TCP API failed: {err}").as_str());
                }
            });
        }
        serve_ipc_local_api(endpoint, app).await
    } else {
        serve_tcp_local_api(app).await
    }
}

fn local_api_app(runtime: LocalRuntime, desktop_auth_token: Option<String>) -> Router {
    let frontend_dist_dir =
        local_frontend_dist_dir().unwrap_or_else(|| PathBuf::from("frontend/dist"));
    let frontend_service = ServeDir::new(frontend_dist_dir.clone())
        .not_found_service(ServeFile::new(frontend_dist_dir.join("index.html")));
    let api_routes = local_api_routes(desktop_auth_token);
    Router::new()
        .merge(api_routes)
        .fallback_service(frontend_service)
        .with_state(runtime)
        .layer(local_api_cors())
}

fn local_api_routes(desktop_auth_token: Option<String>) -> Router<LocalRuntime> {
    Router::new()
        .merge(crate::local_runtime::api::router())
        .route("/api/local/status", get(local_status))
        .route("/api/local/auth/login", post(local_login))
        .route("/api/local/auth/register", post(local_register))
        .route(
            "/api/local/auth/register/send-code",
            post(local_send_register_email_code),
        )
        .route("/api/local/auth/logout", post(local_logout))
        .route("/api/local/auth/desktop-ticket", post(local_desktop_ticket))
        .route("/api/local/fs/list", get(local_fs_list_handler))
        .route("/api/local/workspaces", post(local_add_workspace))
        .route(
            "/api/local/workspaces/{workspace_id}",
            delete(local_remove_workspace),
        )
        .route(
            "/api/local/workspaces/{workspace_id}/project-config-trust",
            post(local_update_workspace_project_config_trust),
        )
        .route(
            "/api/local/commands",
            get(local_command_history).delete(local_clear_command_history),
        )
        .route("/api/local/docker/status", get(local_docker_status))
        .route(
            "/api/local/runtime-settings",
            get(local_runtime_settings).post(local_update_runtime_settings),
        )
        .route(
            "/api/local/agent-prompts/status",
            get(local_agent_prompt_status),
        )
        .route(
            "/api/local/agent-prompts/check",
            post(local_check_agent_prompt_updates),
        )
        .route(
            "/api/local/agent-prompts/update",
            post(local_update_agent_prompt_bundle),
        )
        .route(
            "/api/local/system-permissions",
            get(local_system_permissions),
        )
        .route(
            "/api/local/system-permissions/{permission_id}/request",
            post(local_request_system_permission),
        )
        .route("/api/local/sandbox/toggle", post(local_toggle_sandbox))
        .route(
            "/api/local/sandbox/capabilities",
            get(local_sandbox_capabilities),
        )
        .route(
            "/api/local/sandbox/settings",
            get(local_sandbox_settings).put(local_update_sandbox_settings),
        )
        .route("/api/local/sandbox/images", get(local_sandbox_images))
        .route(
            "/api/local/sandbox/images/{image_id}",
            delete(local_delete_sandbox_image),
        )
        .route(
            "/api/local/sandbox/images/{image_id}/reinitialize",
            post(local_reinitialize_sandbox_image),
        )
        .route(
            "/api/local/sandbox/images/mcp",
            post(local_sandbox_image_mcp),
        )
        .route(
            "/api/local/sandbox/images/jobs",
            get(local_sandbox_image_jobs),
        )
        .route("/api/local/sandbox/leases", get(local_sandbox_leases))
        .route(
            "/api/local/sandbox/images/initialize",
            post(local_initialize_sandbox_image),
        )
        .route("/api/local/terminal/exec", post(local_terminal_exec))
        .route(
            "/api/local/model-configs",
            get(local_model_configs).post(local_save_model_config),
        )
        .route(
            "/api/local/mcp-configs",
            get(local_mcp_configs).post(local_save_mcp_config),
        )
        .route(
            "/api/local/mcp-configs/{manifest_id}",
            get(local_get_mcp_config)
                .post(local_update_mcp_config)
                .delete(local_delete_mcp_config),
        )
        .route(
            "/api/local/mcp-configs/{manifest_id}/test",
            post(local_test_mcp_config),
        )
        .route(
            "/api/local/mcp-configs/{manifest_id}/enable",
            post(local_enable_mcp_config),
        )
        .route(
            "/api/local/mcp-configs/{manifest_id}/disable",
            post(local_disable_mcp_config),
        )
        .route(
            "/api/local/mcp-configs/{manifest_id}/sync",
            post(local_sync_mcp_config),
        )
        .route("/api/local/skills", get(local_skills))
        .route("/api/local/skills/sync", post(local_sync_skill_inventory))
        .route(
            "/api/local/skills/{skill_id}/preference",
            post(local_update_skill_preference),
        )
        .route(
            "/api/local/model-configs/catalog/preview",
            post(local_preview_model_catalog),
        )
        .route(
            "/api/local/model-configs/{id}",
            post(local_update_model_config).delete(local_delete_model_config),
        )
        .route(
            "/api/local/model-configs/{id}/sync",
            post(local_sync_model_config),
        )
        .route(
            "/api/local/model-settings",
            get(local_model_settings).post(local_update_model_settings),
        )
        .route(
            "/api/local/approval/settings",
            get(local_approval_settings).post(local_update_approval_settings),
        )
        .route("/api/local/approval/pending", get(local_pending_approvals))
        .route(
            "/api/local/approval/pending/{id}/approve",
            post(local_approve_pending_approval),
        )
        .route(
            "/api/local/approval/pending/{id}/deny",
            post(local_deny_pending_approval),
        )
        .route_layer(middleware::from_fn_with_state(
            desktop_auth_token,
            require_local_desktop_auth,
        ))
}

async fn serve_tcp_local_api(app: Router) -> Result<()> {
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), local_api_port());
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing_stdout(format!("local connector core API listening on http://{bind_addr}").as_str());
    if should_open_local_ui() {
        let url = format!("http://{bind_addr}");
        tokio::spawn(async move {
            if let Err(err) = open_local_ui(url.as_str()).await {
                tracing_stdout(format!("open local connector UI failed: {err}").as_str());
            }
        });
    }
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(windows)]
async fn serve_ipc_local_api(endpoint: String, app: Router) -> Result<()> {
    use anyhow::Context;
    use tokio::net::windows::named_pipe::ServerOptions;

    tracing_stdout(format!("local connector core API listening on named pipe {endpoint}").as_str());
    loop {
        let server = ServerOptions::new()
            .create(endpoint.as_str())
            .with_context(|| format!("create local connector API named pipe {endpoint}"))?;
        server
            .connect()
            .await
            .with_context(|| format!("accept local connector API named pipe {endpoint}"))?;
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_ipc_connection(server, app).await {
                tracing_stdout(format!("local connector IPC connection failed: {err}").as_str());
            }
        });
    }
}

#[cfg(unix)]
async fn serve_ipc_local_api(endpoint: String, app: Router) -> Result<()> {
    use anyhow::Context;
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    let path = PathBuf::from(endpoint);
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| {
            format!("remove stale local connector API socket {}", path.display())
        })?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("create local connector API socket dir {}", parent.display())
        })?;
    }
    let listener = UnixListener::bind(&path)
        .with_context(|| format!("bind local connector API socket {}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restrict local connector API socket {}", path.display()))?;
    tracing_stdout(
        format!(
            "local connector core API listening on Unix socket {}",
            path.display()
        )
        .as_str(),
    );
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .with_context(|| format!("accept local connector API socket {}", path.display()))?;
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_ipc_connection(stream, app).await {
                tracing_stdout(format!("local connector IPC connection failed: {err}").as_str());
            }
        });
    }
}

async fn serve_ipc_connection<I>(stream: I, app: Router) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let builder = HyperConnectionBuilder::new(TokioExecutor::new());
    builder
        .serve_connection(TokioIo::new(stream), TowerToHyperService::new(app))
        .await
        .map_err(|err| anyhow::anyhow!("serve local connector IPC API connection: {err}"))?;
    Ok(())
}

fn local_frontend_dist_dir() -> Option<PathBuf> {
    if let Some(value) = optional_env("LOCAL_CONNECTOR_FRONTEND_DIST") {
        let path = PathBuf::from(value);
        if path.join("index.html").is_file() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("frontend").join("dist"));
            candidates.push(exe_dir.join("resources").join("frontend").join("dist"));
            if let Some(contents_dir) = exe_dir.parent() {
                candidates.push(contents_dir.join("Resources").join("frontend").join("dist"));
            }
        }
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("frontend").join("dist"));
        candidates.push(
            current_dir
                .join("local_connector_client")
                .join("frontend")
                .join("dist"),
        );
    }
    for ancestor in std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors() {
        candidates.push(ancestor.join("frontend").join("dist"));
        candidates.push(
            ancestor
                .join("local_connector_client")
                .join("frontend")
                .join("dist"),
        );
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.join("index.html").is_file())
}

fn should_open_local_ui() -> bool {
    std::env::args().any(|arg| arg == "--open") || env_flag("LOCAL_CONNECTOR_OPEN_UI")
}

fn env_flag(name: &str) -> bool {
    optional_env(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

async fn open_local_ui(url: &str) -> Result<()> {
    let mut command = match std::env::consts::OS {
        "macos" => {
            let mut command = tokio::process::Command::new("open");
            command.arg(url);
            command
        }
        "windows" => {
            let mut command = tokio::process::Command::new("cmd");
            command.args(["/C", "start", "", url]);
            command
        }
        _ => {
            let mut command = tokio::process::Command::new("xdg-open");
            command.arg(url);
            command
        }
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn()?.wait().await?;
    Ok(())
}

fn local_desktop_auth_token() -> Option<String> {
    optional_env("LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN")
}

fn local_api_port() -> u16 {
    optional_env("LOCAL_CONNECTOR_CORE_API_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_LOCAL_API_PORT)
}

fn local_api_cors() -> CorsLayer {
    let core_port = local_api_port();
    let frontend_port = optional_env("LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(39_233);
    let configured = optional_env("LOCAL_CONNECTOR_DESKTOP_ALLOWED_ORIGINS")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                format!("http://127.0.0.1:{core_port}"),
                format!("http://localhost:{core_port}"),
                format!("http://127.0.0.1:{frontend_port}"),
                format!("http://localhost:{frontend_port}"),
            ]
        });
    let origins = configured
        .into_iter()
        .filter_map(|value| HeaderValue::from_str(value.as_str()).ok())
        .collect::<Vec<_>>();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
}

async fn require_local_desktop_auth(
    State(expected_token): State<Option<String>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.method() == Method::OPTIONS || expected_token.is_none() {
        return next.run(request).await;
    }
    let expected_token = expected_token.unwrap_or_default();
    if bearer_token_matches(request.headers(), expected_token.as_str()) {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": "Local Connector desktop authorization is required"
        })),
    )
        .into_response()
}

fn bearer_token_matches(headers: &HeaderMap, expected_token: &str) -> bool {
    let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    token_digest(token) == token_digest(expected_token)
}

fn token_digest(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}
