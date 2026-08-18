// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::middleware;
use axum::routing::{any, get, post, put};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::state::AppState;

use super::managed_runtime_config::get_managed_runtime_config;
use super::{
    connect_device, create_device, create_local_mcp, create_managed_requirements_assignment,
    create_managed_requirements_policy, create_project_binding, create_sandbox_pairing,
    create_workspace, current_user_handler, delete_local_mcp,
    delete_managed_requirements_assignment, delete_managed_requirements_policy,
    delete_project_binding, delete_sandbox_pairing, delete_workspace, disconnect_device,
    get_agent_prompt_bundle, get_agent_prompt_bundle_manifest, get_device,
    get_managed_requirements, health_handler, heartbeat_device, list_devices, list_local_mcps,
    list_managed_requirements_assignments, list_managed_requirements_policies,
    list_plugin_install_sources, list_project_bindings, list_sandbox_pairings, list_user_skills,
    list_workspaces, mcp_relay, plugin_artifact_create_relay, plugin_artifact_list_relay,
    plugin_artifact_read_relay, plugin_artifact_update_relay, plugin_cancel_relay,
    plugin_execute_relay, plugin_prepare_relay, plugin_ui_asset_relay,
    proxy_plugin_release_artifact, remote_connection_command_relay, remote_connection_test_relay,
    remote_sftp_relay, remote_terminal_close_relay, remote_terminal_ws_relay,
    require_internal_auth, require_public_auth, resolve_local_runtime_capabilities, revoke_device,
    sandbox_facade_path, sandbox_facade_root, skill_cancel_relay, skill_execute_relay,
    skill_prepare_relay, sync_user_skill_inventory, system_stats_handler, terminal_close_relay,
    terminal_exec_relay, terminal_input_relay, terminal_session_create_relay, terminal_ws_relay,
    update_local_mcp, update_local_mcp_status, update_managed_requirements_assignment,
    update_managed_requirements_policy, update_plugin_preference, update_project_binding,
    update_sandbox_pairing, update_user_skill_preference, update_workspace,
    user_service_protected_proxy, user_service_public_proxy, workspace_directory_create_relay,
    workspace_directory_list_relay, workspace_filesystem_relay, AuthState,
};

fn protected_api(state: &AppState, internal: bool) -> Router<AppState> {
    let auth_state = AuthState::from_app_state(state);
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
            "/api/local-connectors/devices/{id}/managed-requirements",
            get(get_managed_requirements),
        )
        .route(
            "/api/local-connectors/managed-requirements/policies",
            get(list_managed_requirements_policies).post(create_managed_requirements_policy),
        )
        .route(
            "/api/local-connectors/managed-requirements/policies/{id}",
            put(update_managed_requirements_policy).delete(delete_managed_requirements_policy),
        )
        .route(
            "/api/local-connectors/managed-requirements/assignments",
            get(list_managed_requirements_assignments).post(create_managed_requirements_assignment),
        )
        .route(
            "/api/local-connectors/managed-requirements/assignments/{id}",
            put(update_managed_requirements_assignment)
                .delete(delete_managed_requirements_assignment),
        )
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
            "/api/local-connectors/system/stats",
            get(system_stats_handler),
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
            "/api/local-connectors/relay/{device_id}/plugins/prepare",
            post(plugin_prepare_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/plugins/execute",
            post(plugin_execute_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/plugins/cancel",
            post(plugin_cancel_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/plugins/ui/assets",
            post(plugin_ui_asset_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/workspaces/{workspace_id}/directories",
            get(workspace_directory_list_relay).post(workspace_directory_create_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/workspaces/{workspace_id}/filesystem",
            post(workspace_filesystem_relay),
        )
        .merge(plugin_artifact_routes())
        .route(
            "/api/plugin-management/agent-capabilities/{agent_key}",
            get(resolve_local_runtime_capabilities),
        )
        .route(
            "/api/plugin-management/agent-prompts/manifest",
            get(get_agent_prompt_bundle_manifest),
        )
        .route(
            "/api/plugin-management/agent-prompts/bundle",
            get(get_agent_prompt_bundle),
        )
        .route(
            "/api/local-connectors/config/runtime",
            get(get_managed_runtime_config),
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
            "/api/plugin-management/plugins/install-sources",
            get(list_plugin_install_sources),
        )
        .route(
            "/api/plugin-management/plugins/{plugin_id}/preference",
            axum::routing::put(update_plugin_preference),
        )
        .route(
            "/api/plugin-management/plugins/{plugin_id}/releases/{release_id}/artifact",
            get(proxy_plugin_release_artifact),
        )
        .route(
            "/api/plugin-management/skills/inventory",
            put(sync_user_skill_inventory),
        )
        .route(
            "/api/plugin-management/skills/{skill_id}/preference",
            put(update_user_skill_preference),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/exec",
            post(terminal_exec_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/remote-connections/test",
            post(remote_connection_test_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/remote-connections/command",
            post(remote_connection_command_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/remote-connections/sftp",
            post(remote_sftp_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/remote-connections/terminal/ws",
            get(remote_terminal_ws_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/remote-connections/terminal/close",
            post(remote_terminal_close_relay),
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
            "/api/local-connectors/relay/{device_id}/terminal/close",
            post(terminal_close_relay),
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
        );

    if internal {
        protected_api.route_layer(middleware::from_fn_with_state(
            auth_state,
            require_internal_auth,
        ))
    } else {
        protected_api.route_layer(middleware::from_fn_with_state(
            auth_state,
            require_public_auth,
        ))
    }
}

pub fn build_public_router(state: AppState) -> Router {
    apply_common_layers(
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
            .merge(protected_api(&state, false))
            .with_state(state),
    )
}

pub fn build_internal_router(state: AppState) -> Router {
    apply_common_layers(
        Router::new()
            .route("/api/health", get(health_handler))
            .merge(protected_api(&state, true))
            .with_state(state),
    )
}

fn apply_common_layers(router: Router) -> Router {
    router
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
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

fn plugin_artifact_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    super::PluginArtifactRelayState: axum::extract::FromRef<S>,
{
    Router::new()
        .route(
            "/api/local-connectors/relay/{device_id}/plugins/artifacts/list",
            post(plugin_artifact_list_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/plugins/artifacts/read",
            post(plugin_artifact_read_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/plugins/artifacts/create",
            post(plugin_artifact_create_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/plugins/artifacts/update",
            post(plugin_artifact_update_relay),
        )
}

#[cfg(feature = "test-support")]
pub fn build_plugin_artifact_relay_test_router(
    config: crate::config::AppConfig,
    relay: crate::relay::ConnectorRelay,
    scope: super::PluginArtifactRelayTestScope,
) -> Result<Router, String> {
    let auth_state = AuthState::for_test(config.clone())?;
    let relay_state = super::PluginArtifactRelayState::for_test(
        relay,
        config.relay_request_timeout,
        config.plugin_hook_relay_request_timeout,
        scope,
    );
    Ok(plugin_artifact_routes::<super::PluginArtifactRelayState>()
        .route_layer(middleware::from_fn_with_state(
            auth_state,
            require_internal_auth,
        ))
        .with_state(relay_state))
}

#[cfg(feature = "test-support")]
pub fn build_plugin_artifact_relay_store_test_router(
    config: crate::config::AppConfig,
    relay: crate::relay::ConnectorRelay,
    store: crate::store::ConnectorStore,
) -> Result<Router, String> {
    let auth_state = AuthState::for_test(config.clone())?;
    let relay_state = super::PluginArtifactRelayState::for_store_test(
        relay,
        config.relay_request_timeout,
        config.plugin_hook_relay_request_timeout,
        store,
    );
    Ok(plugin_artifact_routes::<super::PluginArtifactRelayState>()
        .route_layer(middleware::from_fn_with_state(
            auth_state,
            require_internal_auth,
        ))
        .with_state(relay_state))
}
