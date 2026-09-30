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
use super::metrics::{health_handler, prometheus_metrics};
use super::{
    connect_device, controlled_network_readiness, create_device,
    create_managed_requirements_assignment, create_managed_requirements_policy,
    create_project_binding, create_workspace, current_user_handler,
    delete_managed_requirements_assignment, delete_managed_requirements_policy,
    delete_project_binding, delete_workspace, disconnect_device, get_agent_prompt_bundle,
    get_agent_prompt_bundle_manifest, get_device, get_managed_requirements, heartbeat_device,
    list_devices, list_managed_requirements_assignments, list_managed_requirements_policies,
    list_plugin_install_sources, list_project_bindings, list_workspaces,
    proxy_plugin_release_artifact, require_internal_auth, require_public_auth,
    resolve_local_runtime_capabilities, revoke_device, system_stats_handler,
    update_managed_requirements_assignment, update_managed_requirements_policy,
    update_plugin_preference, update_project_binding, update_workspace,
    user_service_protected_proxy, user_service_public_proxy, AuthState,
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
        .route("/api/model-providers", any(user_service_protected_proxy))
        .route(
            "/api/model-providers/{*path}",
            any(user_service_protected_proxy),
        )
        .route(
            "/api/local-connectors/devices",
            get(list_devices).post(create_device),
        )
        .route(
            "/api/local-connectors/companion/devices",
            get(super::devices::list_companion_devices),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/resources",
            get(super::companion::list_companion_resources),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/resources/resolve",
            post(super::companion::resolve_companion_resource),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/agent-workspace",
            get(super::companion::get_companion_agent_workspace),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/agent-conversations/{conversation_id}",
            get(super::companion::get_companion_agent_conversation),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/agent-conversations/{conversation_id}/messages",
            get(super::companion::list_companion_agent_messages)
                .post(super::companion::send_companion_agent_message),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/agents/{agent_id}/direct-conversation",
            post(super::companion::open_companion_agent_direct_conversation),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/approvals",
            get(super::companion::list_companion_approvals),
        )
        .route(
            "/api/local-connectors/companion/devices/{device_id}/approvals/{approval_id}/resolve",
            post(super::companion::resolve_companion_approval),
        )
        .route("/api/local-connectors/devices/{id}", get(get_device))
        .route(
            "/api/local-connectors/devices/{id}/controlled-network/readiness",
            get(controlled_network_readiness),
        )
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
        ;

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
            .route("/metrics", get(prometheus_metrics))
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
