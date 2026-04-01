use axum::{
    routing::{delete, get, post},
    Router,
};

mod change_handlers;
mod change_support;
mod contact_handlers;
mod contracts;
mod crud_handlers;
mod memory_sync;
mod run_handlers;

use self::change_handlers::{
    confirm_project_changes, get_project_change_summary, list_project_changes,
};
use self::contact_handlers::{add_project_contact, list_project_contacts, remove_project_contact};
use self::crud_handlers::{
    create_project, delete_project, get_project, list_projects, update_project,
};
use self::run_handlers::{
    analyze_project_run, execute_project_run, get_project_run_catalog, set_project_run_default,
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
        .route("/api/projects/:id/changes", get(list_project_changes))
        .route(
            "/api/projects/:id/changes/summary",
            get(get_project_change_summary),
        )
        .route(
            "/api/projects/:id/changes/confirm",
            post(confirm_project_changes),
        )
        .route("/api/projects/:id/run/analyze", post(analyze_project_run))
        .route("/api/projects/:id/run/catalog", get(get_project_run_catalog))
        .route("/api/projects/:id/run/execute", post(execute_project_run))
        .route("/api/projects/:id/run/default", post(set_project_run_default))
}
