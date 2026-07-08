// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

use crate::config::{optional_env, DEFAULT_LOCAL_API_PORT};
use crate::{tracing_stdout, LocalRuntime};

mod handlers;
mod types;

use handlers::{
    local_add_workspace, local_clear_command_history, local_command_history, local_docker_status,
    local_fs_list_handler, local_initialize_sandbox_image, local_login, local_logout,
    local_register, local_remove_workspace, local_sandbox_image_jobs, local_sandbox_images,
    local_sandbox_leases, local_status, local_terminal_exec, local_toggle_sandbox,
};

pub(crate) async fn serve_local_api(runtime: LocalRuntime) -> Result<()> {
    let bind_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        optional_env("LOCAL_CONNECTOR_CORE_API_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(DEFAULT_LOCAL_API_PORT),
    );
    let app = Router::new()
        .route("/api/local/status", get(local_status))
        .route("/api/local/auth/login", post(local_login))
        .route("/api/local/auth/register", post(local_register))
        .route("/api/local/auth/logout", post(local_logout))
        .route("/api/local/fs/list", get(local_fs_list_handler))
        .route("/api/local/workspaces", post(local_add_workspace))
        .route(
            "/api/local/workspaces/{workspace_id}",
            delete(local_remove_workspace),
        )
        .route(
            "/api/local/commands",
            get(local_command_history).delete(local_clear_command_history),
        )
        .route("/api/local/docker/status", get(local_docker_status))
        .route("/api/local/sandbox/toggle", post(local_toggle_sandbox))
        .route("/api/local/sandbox/images", get(local_sandbox_images))
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
        .with_state(runtime)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing_stdout(format!("local connector core API listening on http://{bind_addr}").as_str());
    axum::serve(listener, app).await?;
    Ok(())
}
