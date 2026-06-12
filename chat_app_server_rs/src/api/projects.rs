use axum::{
    Router,
    routing::{delete, get, post},
};

mod contact_handlers;
mod contracts;
mod crud_handlers;
mod memory_sync;
mod run_handlers;

use self::contact_handlers::{add_project_contact, list_project_contacts, remove_project_contact};
use self::crud_handlers::{
    create_project, delete_project, get_project, list_projects, update_project,
};
use self::run_handlers::{
    analyze_project_run, execute_project_run, get_project_run_catalog, get_project_run_environment,
    get_project_run_state, set_project_run_default, update_project_run_environment,
};

pub fn router() -> Router {
    Router::new()
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:id",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route(
            "/api/projects/:id/contacts",
            get(list_project_contacts).post(add_project_contact),
        )
        .route(
            "/api/projects/:id/contacts/:contact_id",
            delete(remove_project_contact),
        )
        .route("/api/projects/:id/run/analyze", post(analyze_project_run))
        .route(
            "/api/projects/:id/run/catalog",
            get(get_project_run_catalog),
        )
        .route("/api/projects/:id/run/execute", post(execute_project_run))
        .route("/api/projects/:id/run/state", get(get_project_run_state))
        .route(
            "/api/projects/:id/run/default",
            post(set_project_run_default),
        )
        .route(
            "/api/projects/:id/run/environment",
            get(get_project_run_environment).put(update_project_run_environment),
        )
}
