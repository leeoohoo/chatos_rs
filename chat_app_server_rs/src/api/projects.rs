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

use self::change_handlers::{
    confirm_project_changes, get_project_change_summary, list_project_changes,
};
use self::contact_handlers::{add_project_contact, list_project_contacts, remove_project_contact};
use self::crud_handlers::{
    create_project, delete_project, get_project, list_projects, update_project,
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
}
