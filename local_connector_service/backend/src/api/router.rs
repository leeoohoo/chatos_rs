// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::middleware;
use axum::routing::{any, get, post, put};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::state::AppState;

use super::{
    connect_device, create_device, create_local_mcp, create_project_binding,
    create_sandbox_pairing, create_workspace, current_user_handler, delete_local_mcp,
    delete_project_binding, delete_sandbox_pairing, delete_workspace, disconnect_device,
    get_device, health_handler, heartbeat_device, list_devices, list_local_mcps,
    list_project_bindings, list_sandbox_pairings, list_user_skills, list_workspaces, mcp_relay,
    memory_engine_proxy, require_auth, resolve_local_command_approval_capabilities,
    resolve_model_runtime, revoke_device, sandbox_facade_path, sandbox_facade_root,
    skill_cancel_relay, skill_execute_relay, skill_prepare_relay, sync_user_skill_inventory,
    terminal_exec_relay, terminal_input_relay, terminal_session_create_relay, terminal_ws_relay,
    update_local_mcp, update_local_mcp_status, update_project_binding, update_sandbox_pairing,
    update_user_skill_preference, update_workspace, user_service_protected_proxy,
    user_service_public_proxy,
};

pub fn build_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/model-configs", any(user_service_protected_proxy))
        .route(
            "/api/model-configs/{*path}",
            any(user_service_protected_proxy),
        )
        .route(
            "/api/local-connectors/devices",
            get(list_devices).post(create_device),
        )
        .route("/api/local-connectors/devices/{id}", get(get_device))
        .route(
            "/api/local-connectors/devices/{id}/heartbeat",
            post(heartbeat_device),
        )
        .route(
            "/api/local-connectors/devices/{id}/revoke",
            post(revoke_device),
        )
        .route(
            "/api/local-connectors/devices/{id}/disconnect",
            post(disconnect_device),
        )
        .route(
            "/api/local-connectors/devices/{id}/connect",
            get(connect_device),
        )
        .route(
            "/api/local-connectors/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/api/local-connectors/workspaces/{id}",
            put(update_workspace).delete(delete_workspace),
        )
        .route(
            "/api/local-connectors/project-bindings",
            get(list_project_bindings).post(create_project_binding),
        )
        .route(
            "/api/local-connectors/project-bindings/{id}",
            put(update_project_binding).delete(delete_project_binding),
        )
        .route(
            "/api/local-connectors/sandbox-pairings",
            get(list_sandbox_pairings).post(create_sandbox_pairing),
        )
        .route(
            "/api/local-connectors/sandbox-pairings/{id}",
            put(update_sandbox_pairing).delete(delete_sandbox_pairing),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/mcp",
            post(mcp_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/skills/prepare",
            post(skill_prepare_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/skills/execute",
            post(skill_execute_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/skills/cancel",
            post(skill_cancel_relay),
        )
        .route(
            "/api/local-connectors/model-runtime/{model_config_id}",
            get(resolve_model_runtime),
        )
        .route(
            "/api/plugin-management/agent-capabilities/local-command-approval",
            get(resolve_local_command_approval_capabilities),
        )
        .route(
            "/api/plugin-management/local-mcps",
            get(list_local_mcps).post(create_local_mcp),
        )
        .route(
            "/api/plugin-management/local-mcps/{mcp_id}",
            axum::routing::patch(update_local_mcp).delete(delete_local_mcp),
        )
        .route(
            "/api/plugin-management/local-mcps/{mcp_id}/status",
            put(update_local_mcp_status),
        )
        .route("/api/plugin-management/skills", get(list_user_skills))
        .route(
            "/api/plugin-management/skills/inventory",
            put(sync_user_skill_inventory),
        )
        .route(
            "/api/plugin-management/skills/{skill_id}/preference",
            put(update_user_skill_preference),
        )
        .route(
            "/api/local-connectors/memory-engine/{*path}",
            any(memory_engine_proxy),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/exec",
            post(terminal_exec_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/sessions",
            post(terminal_session_create_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/input",
            post(terminal_input_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/ws",
            get(terminal_ws_relay),
        )
        .route(
            "/api/local-connectors/sandbox-facade/{pairing_id}",
            any(sandbox_facade_root),
        )
        .route(
            "/api/local-connectors/sandbox-facade/{pairing_id}/{*path}",
            any(sandbox_facade_path),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/auth/login", post(user_service_public_proxy))
        .route("/api/auth/register", post(user_service_public_proxy))
        .route(
            "/api/auth/register/send-code",
            post(user_service_public_proxy),
        )
        .route(
            "/api/auth/local-connector-ticket/exchange",
            post(user_service_public_proxy),
        )
        .merge(protected_api)
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::DEBUG))
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}
