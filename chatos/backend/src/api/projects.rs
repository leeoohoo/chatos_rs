// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    routing::{delete, get, post, put},
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
mod runtime_environment_handlers;
mod session_resolver;

use self::contact_handlers::{
    add_project_contact, get_project_contact_lock, list_project_contacts, remove_project_contact,
};
use self::crud_handlers::{
    create_cloud_project, create_project, delete_project, get_project, list_projects,
    update_project,
};
use self::plan_handlers::{
    get_project_plan, list_requirement_documents, list_requirement_work_items,
};
use self::requirement_execution_handlers::{execute_requirement, stop_requirement_execution};
use self::run_handlers::{
    analyze_project_run, execute_project_run, get_project_run_catalog, get_project_run_environment,
    get_project_run_state, set_project_run_default, update_project_run_environment,
};
use self::runtime_environment_handlers::{
    analyze_project_runtime_environment, generate_project_runtime_environment_image,
    get_project_runtime_environment, get_project_runtime_environment_progress,
    update_project_runtime_environment_settings,
};

pub fn router() -> Router {
    Router::new()
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/projects/cloud", post(create_cloud_project))
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
            "/api/projects/{id}/requirements/{requirement_id}/stop",
            post(stop_requirement_execution),
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
        .route(
            "/api/projects/{id}/runtime-environment",
            get(get_project_runtime_environment),
        )
        .route(
            "/api/projects/{id}/runtime-environment/settings",
            put(update_project_runtime_environment_settings),
        )
        .route(
            "/api/projects/{id}/runtime-environment/analyze",
            post(analyze_project_runtime_environment),
        )
        .route(
            "/api/projects/{id}/runtime-environment/images/{image_record_id}/generate",
            post(generate_project_runtime_environment_image),
        )
        .route(
            "/api/projects/{id}/runtime-environment/progress",
            get(get_project_runtime_environment_progress),
        )
}
