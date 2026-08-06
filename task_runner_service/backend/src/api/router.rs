// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::middleware;
use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use super::core::{
    agent_token_handler, create_user, current_user_handler, delete_user, health_handler,
    list_users, login_handler, logout_handler, require_auth, sse_ticket_handler,
    system_config_handler, task_runner_internal_prompt_preview_handler,
    update_system_config_handler, update_user,
};
use super::internal::{
    get_system_stats, get_user_execution_options, prometheus_metrics, replay_run_post_process,
};
use super::internal_auth::{
    require_task_runner_internal_request, CHATOS_CALLER, MODEL_CONFIGS_SYNC_SCOPE,
    PROJECTS_SYNC_SCOPE, USER_SERVICE_CALLER,
};
use super::mcp::{
    get_mcp_provider_descriptor, get_mcp_server_info, list_mcp_catalog, list_plugin_connectors,
    list_task_capability_catalog, mcp_entrypoint, mcp_management_entrypoint, preview_mcp_prompt,
};
use super::models::{
    create_model_config, delete_model_config, get_model_config, list_model_catalog,
    list_model_config_usage, list_model_configs, preview_model_catalog, test_model_config,
    update_model_config,
};
use super::projects::{
    create_project, delete_project, get_project, get_project_runtime_environment,
    import_chatos_project, list_project_tasks, list_projects, sync_get_project, sync_list_projects,
    update_project,
};
use super::prompts::{
    cancel_prompt, get_prompt, list_prompt_task_counts, list_prompts, list_prompts_page,
    list_run_prompts, submit_prompt,
};
use super::remote_servers::{
    create_remote_server, delete_remote_server, get_remote_server, list_remote_servers,
    test_remote_server_draft, test_remote_server_saved, update_remote_server,
};
use super::runs::{
    cancel_run, get_run, get_run_output_changes, get_run_output_diff, list_run_events,
    list_run_index, list_run_summaries, list_runs, list_runs_page, list_task_runs, retry_run,
    start_task_run, stream_run_events,
};
use super::tasks::{
    batch_delete_tasks, batch_start_task_runs, batch_update_task_status, cancel_task, create_task,
    delete_task, get_task, get_task_dependency_graph, get_task_index, get_task_mcp_resolution,
    get_task_memory_context, get_task_memory_records, get_task_stats, list_task_prerequisites,
    list_task_summaries, list_tasks, list_tasks_page, preview_task_mcp_prompt, record_task_process,
    set_task_prerequisites, summarize_task_memory, update_task,
};
use super::tooling::{
    get_terminal_process_logs, kill_terminal_process, list_notepad_folders, list_notepad_notes,
    list_notepad_tags, list_terminal_processes, read_notepad_note, write_terminal_process,
};
use super::*;
use crate::models::{ChatosSyncedModelConfigRequest, ModelConfigRecord};

pub fn build_public_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/auth/sse-ticket", post(sse_ticket_handler))
        .route("/api/system/config", patch(update_system_config_handler))
        .route(
            "/api/system/internal-prompts",
            get(task_runner_internal_prompt_preview_handler),
        )
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", patch(update_user).delete(delete_user))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route("/api/projects/{id}/tasks", get(list_project_tasks))
        .route(
            "/api/projects/{id}/runtime-environment",
            get(get_project_runtime_environment),
        )
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/summaries", get(list_task_summaries))
        .route("/api/tasks/page", get(list_tasks_page))
        .route("/api/tasks/index", get(get_task_index))
        .route("/api/tasks/stats", get(get_task_stats))
        .route("/api/tasks/batch/status", post(batch_update_task_status))
        .route("/api/tasks/batch/delete", post(batch_delete_tasks))
        .route("/api/tasks/batch/runs", post(batch_start_task_runs))
        .route(
            "/api/tasks/{id}",
            get(get_task).patch(update_task).delete(delete_task),
        )
        .route("/api/tasks/{id}/cancel", post(cancel_task))
        .route(
            "/api/tasks/{id}/runs",
            get(list_task_runs).post(start_task_run),
        )
        .route(
            "/api/tasks/{id}/mcp/resolution",
            get(get_task_mcp_resolution),
        )
        .route(
            "/api/tasks/{id}/prerequisites",
            get(list_task_prerequisites).put(set_task_prerequisites),
        )
        .route(
            "/api/tasks/{id}/dependency-graph",
            get(get_task_dependency_graph),
        )
        .route("/api/tasks/{id}/process-log", patch(record_task_process))
        .route(
            "/api/tasks/{id}/mcp/prompt-preview",
            get(preview_task_mcp_prompt),
        )
        .route(
            "/api/tasks/{id}/memory/context",
            get(get_task_memory_context),
        )
        .route(
            "/api/tasks/{id}/memory/records",
            get(get_task_memory_records),
        )
        .route(
            "/api/tasks/{id}/memory/summarize",
            post(summarize_task_memory),
        )
        .route(
            "/api/model-configs",
            get(list_model_configs).post(create_model_config),
        )
        .route(
            "/api/model-configs/catalog/preview",
            post(preview_model_catalog),
        )
        .route(
            "/api/model-configs/{id}",
            get(get_model_config)
                .patch(update_model_config)
                .delete(delete_model_config),
        )
        .route("/api/model-configs/{id}/models", get(list_model_catalog))
        .route("/api/model-configs/{id}/test", post(test_model_config))
        .route("/api/model-configs/usage", get(list_model_config_usage))
        .route(
            "/api/remote-servers",
            get(list_remote_servers).post(create_remote_server),
        )
        .route("/api/remote-servers/test", post(test_remote_server_draft))
        .route(
            "/api/remote-servers/{id}",
            get(get_remote_server)
                .patch(update_remote_server)
                .delete(delete_remote_server),
        )
        .route(
            "/api/remote-servers/{id}/test",
            post(test_remote_server_saved),
        )
        .route("/api/runs", get(list_runs))
        .route("/api/runs/summaries", get(list_run_summaries))
        .route("/api/runs/page", get(list_runs_page))
        .route("/api/runs/index", get(list_run_index))
        .route("/api/runs/{id}", get(get_run))
        .route("/api/runs/{id}/events", get(list_run_events))
        .route("/api/runs/{id}/output/changes", get(get_run_output_changes))
        .route("/api/runs/{id}/output/diff", get(get_run_output_diff))
        .route("/api/runs/{id}/prompts", get(list_run_prompts))
        .route("/api/runs/{id}/stream", get(stream_run_events))
        .route("/api/runs/{id}/cancel", post(cancel_run))
        .route("/api/runs/{id}/retry", post(retry_run))
        .route(
            "/api/queue-operations/run-post-process/replay",
            post(replay_run_post_process),
        )
        .route("/api/prompts", get(list_prompts))
        .route("/api/prompts/page", get(list_prompts_page))
        .route("/api/prompts/task-counts", get(list_prompt_task_counts))
        .route("/api/prompts/{id}", get(get_prompt))
        .route("/api/prompts/{id}/submit", post(submit_prompt))
        .route("/api/prompts/{id}/cancel", post(cancel_prompt))
        .route("/api/tooling/notepad/folders", get(list_notepad_folders))
        .route("/api/tooling/notepad/tags", get(list_notepad_tags))
        .route("/api/tooling/notepad/notes", get(list_notepad_notes))
        .route("/api/tooling/notepad/notes/{id}", get(read_notepad_note))
        .route(
            "/api/tooling/terminal/processes",
            get(list_terminal_processes),
        )
        .route(
            "/api/tooling/terminal/processes/{id}/logs",
            get(get_terminal_process_logs),
        )
        .route(
            "/api/tooling/terminal/processes/{id}/kill",
            post(kill_terminal_process),
        )
        .route(
            "/api/tooling/terminal/processes/{id}/write",
            post(write_terminal_process),
        )
        .route("/api/mcp/server", get(get_mcp_server_info))
        .route("/api/mcp/tools", get(list_mcp_catalog))
        .route(
            "/api/tasks/capabilities/catalog",
            get(list_task_capability_catalog),
        )
        .route("/api/tasks/plugin-connectors", get(list_plugin_connectors))
        .route("/api/mcp/prompt-preview", post(preview_mcp_prompt))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/metrics", get(prometheus_metrics))
        .route(
            "/api/mcp/provider-descriptor",
            get(get_mcp_provider_descriptor),
        )
        .route("/api/system/config", get(system_config_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/agent-token", post(agent_token_handler))
        .merge(protected_api)
        .route("/mcp", post(mcp_entrypoint))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(middleware::from_fn(
            crate::trace_context::accept_remote_parent,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                    http_server_span(request, "public")
                })
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

pub fn build_internal_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/chatos-sync/model-configs",
            post(chatos_sync_upsert_model_config),
        )
        .route(
            "/api/chatos-sync/projects",
            get(sync_list_projects).post(import_chatos_project),
        )
        .route("/api/chatos-sync/projects/{id}", get(sync_get_project))
        .route(
            "/api/chatos-sync/model-configs/{id}",
            delete(chatos_sync_delete_model_config),
        )
        .route(
            "/internal/users/{owner_user_id}/execution-options",
            get(get_user_execution_options),
        )
        .route("/internal/system/stats", get(get_system_stats))
        .route(
            "/internal/mcp-management/mcp/{system_key}",
            post(mcp_management_entrypoint),
        )
        .merge(chatos_internal::router())
        .with_state(state)
        .layer(middleware::from_fn(
            crate::trace_context::accept_remote_parent,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                    http_server_span(request, "internal")
                })
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

fn http_server_span(
    request: &axum::http::Request<axum::body::Body>,
    surface: &'static str,
) -> tracing::Span {
    let route = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(axum::extract::MatchedPath::as_str)
        .unwrap_or("/unmatched");
    tracing::info_span!(
        "http.request",
        otel.kind = "server",
        otel.name = %format!("{} {route}", request.method()),
        http.request.method = %request.method(),
        http.route = route,
        surface
    )
}

pub(super) fn require_chatos_project_sync(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<(), ApiError> {
    require_task_runner_internal_request(
        &state.config,
        headers,
        &[CHATOS_CALLER],
        PROJECTS_SYNC_SCOPE,
    )
    .map(|_| ())
    .map_err(|error| ApiError {
        status: error.status,
        message: error.message,
    })
}

async fn chatos_sync_upsert_model_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatosSyncedModelConfigRequest>,
) -> Result<Json<ModelConfigRecord>, ApiError> {
    require_task_runner_internal_request(
        &state.config,
        &headers,
        &[USER_SERVICE_CALLER],
        MODEL_CONFIGS_SYNC_SCOPE,
    )
    .map_err(|error| ApiError {
        status: error.status,
        message: error.message,
    })?;
    let record = state
        .model_config_service
        .upsert_chatos_model_config(request)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(record))
}

async fn chatos_sync_delete_model_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_task_runner_internal_request(
        &state.config,
        &headers,
        &[USER_SERVICE_CALLER],
        MODEL_CONFIGS_SYNC_SCOPE,
    )
    .map_err(|error| ApiError {
        status: error.status,
        message: error.message,
    })?;
    let deleted = state
        .model_config_service
        .delete_model_config(id.trim())
        .await
        .map_err(ApiError::internal)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("model config not found"))
    }
}
