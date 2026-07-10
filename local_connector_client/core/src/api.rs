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
    local_add_workspace, local_approval_settings, local_approve_pending_approval,
    local_clear_command_history, local_command_history, local_delete_mcp_config,
    local_delete_model_config, local_deny_pending_approval, local_disable_mcp_config,
    local_docker_status, local_enable_mcp_config, local_fs_list_handler, local_get_mcp_config,
    local_initialize_sandbox_image, local_login, local_logout, local_mcp_configs,
    local_model_configs, local_model_settings, local_pending_approvals,
    local_preview_model_catalog, local_register, local_remove_workspace, local_runtime_settings,
    local_sandbox_image_jobs, local_sandbox_image_mcp, local_sandbox_images, local_sandbox_leases,
    local_save_mcp_config, local_save_model_config, local_status, local_sync_mcp_config,
    local_sync_model_config, local_terminal_exec, local_test_mcp_config, local_toggle_sandbox,
    local_update_approval_settings, local_update_mcp_config, local_update_model_config,
    local_update_model_settings, local_update_runtime_settings,
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
        .route(
            "/api/local/runtime-settings",
            get(local_runtime_settings).post(local_update_runtime_settings),
        )
        .route("/api/local/sandbox/toggle", post(local_toggle_sandbox))
        .route("/api/local/sandbox/images", get(local_sandbox_images))
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
