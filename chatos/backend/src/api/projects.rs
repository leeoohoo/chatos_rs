// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    routing::{delete, get, post},
    Router,
};

mod contact_handlers;
mod contracts;
mod crud_handlers;
pub(crate) mod memory_sync;
mod plan_handlers;
mod requirement_execution;
mod requirement_execution_handlers;
mod run_handlers;
mod session_resolver;

pub(crate) use self::requirement_execution_handlers::{
    cloud_execution_planner_message_is_stale, execution_message_status,
};

pub(crate) async fn repair_stale_cloud_execution_planner_message_for_reconciler(
    message: crate::models::message::Message,
    no_execution_links: bool,
) -> Result<crate::models::message::Message, String> {
    self::requirement_execution_handlers::repair_stale_cloud_execution_planner_message(
        message,
        no_execution_links,
    )
    .await
    .map_err(|err| format!("{err:?}"))
}

pub(crate) async fn reconcile_requirement_planner_owner_context(
    context: serde_json::Value,
) -> Result<(), String> {
    self::requirement_execution_handlers::reconcile_requirement_planner_owner_context(context)
        .await
        .map_err(|error| format!("{error:?}"))
}

use self::contact_handlers::{
    add_project_contact, get_project_contact_lock, list_project_contacts, remove_project_contact,
};
use self::crud_handlers::{delete_project, get_project, list_projects, update_project};
use self::plan_handlers::{
    get_project_plan, list_requirement_documents, list_requirement_work_items,
};
use self::requirement_execution_handlers::{
    confirm_requirement_execution, execute_requirement, get_requirement_execution_plan,
    pause_requirement_execution, rerun_requirement_execution, resume_requirement_execution,
    stop_requirement_execution,
};
use self::run_handlers::{
    analyze_project_run, execute_project_run, get_project_run_catalog, get_project_run_environment,
    get_project_run_state, set_project_run_default, update_project_run_environment,
};
pub fn router() -> Router {
    Router::new()
        .route("/api/projects", get(list_projects))
        .route(
            "/api/projects/{id}",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route("/api/projects/{id}/plan", get(get_project_plan))
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/work-items",
            get(list_requirement_work_items),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/documents",
            get(list_requirement_documents),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/execute",
            post(execute_requirement),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/execution-plan",
            get(get_requirement_execution_plan),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/confirm-execution",
            post(confirm_requirement_execution),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/pause",
            post(pause_requirement_execution),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/resume",
            post(resume_requirement_execution),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/stop",
            post(stop_requirement_execution),
        )
        .route(
            "/api/projects/{id}/requirements/{requirement_id}/rerun",
            post(rerun_requirement_execution),
        )
        .route(
            "/api/projects/{id}/contacts",
            get(list_project_contacts).post(add_project_contact),
        )
        .route(
            "/api/projects/{id}/contacts/lock",
            get(get_project_contact_lock),
        )
        .route(
            "/api/projects/{id}/contacts/{contact_id}",
            delete(remove_project_contact),
        )
        .route("/api/projects/{id}/run/analyze", post(analyze_project_run))
        .route(
            "/api/projects/{id}/run/catalog",
            get(get_project_run_catalog),
        )
        .route("/api/projects/{id}/run/execute", post(execute_project_run))
        .route("/api/projects/{id}/run/state", get(get_project_run_state))
        .route(
            "/api/projects/{id}/run/default",
            post(set_project_run_default),
        )
        .route(
            "/api/projects/{id}/run/environment",
            get(get_project_run_environment).put(update_project_run_environment),
        )
}
