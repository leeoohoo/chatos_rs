// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod ask_user;
mod chat;
mod context;
mod error;
mod events;
mod filesystem;
mod git;
mod guidance;
mod health;
mod messages;
mod project_management;
mod project_runs;
mod projects;
mod recalls;
mod review_repair;
mod runtime_environment;
mod runtime_settings;
mod sessions;
mod summaries;
mod task_board;
mod task_runs;
mod tools;
mod turn_control;
mod workspace_path;
mod workspaces;

use axum::routing::{get, post};
use axum::Router;

use crate::LocalRuntime;

pub(crate) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route("/api/local/runtime/health", get(health::health))
        .route("/api/local/runtime/chat/send", post(chat::send_chat))
        .merge(ask_user::router())
        .route(
            "/api/local/runtime/chat/guidance",
            post(guidance::send_guidance),
        )
        .route(
            "/api/local/runtime/chat/stop",
            post(turn_control::stop_chat),
        )
        .route(
            "/api/local/runtime/projects",
            get(projects::list_projects).post(projects::create_project),
        )
        .route("/api/local/runtime/devices", get(workspaces::list_devices))
        .route(
            "/api/local/runtime/workspaces",
            get(workspaces::list_workspaces),
        )
        .route(
            "/api/local/runtime/workspaces/{workspace_id}/directories",
            get(workspaces::list_directory).post(workspaces::create_directory),
        )
        .route(
            "/api/local/runtime/projects/{project_id}",
            get(projects::get_project)
                .put(projects::upsert_project)
                .delete(projects::delete_project),
        )
        .merge(project_management::router())
        .merge(project_runs::router())
        .merge(runtime_environment::router())
        .merge(filesystem::router())
        .merge(git::router())
        .route(
            "/api/local/runtime/sessions",
            get(sessions::list_sessions).post(sessions::create_session),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}",
            get(sessions::get_session),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/messages",
            get(messages::list_messages),
        )
        .merge(task_board::router())
        .merge(task_runs::router())
        .route(
            "/api/local/runtime/sessions/{session_id}/events",
            get(events::list_events),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/summaries",
            get(summaries::list_summaries).delete(summaries::clear_summaries),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/summaries/{summary_id}",
            axum::routing::delete(summaries::delete_summary),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/review-repair",
            get(review_repair::review_status).post(review_repair::run_review),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/memory-recalls",
            get(recalls::list_recalls),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/memory-recalls/{recall_id}",
            axum::routing::delete(recalls::forget_recall),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/runtime-settings",
            get(runtime_settings::get_runtime_settings)
                .put(runtime_settings::update_runtime_settings),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/tools",
            get(tools::get_agent_tools),
        )
}
