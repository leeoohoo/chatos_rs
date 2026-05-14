use axum::Router;

use crate::api;

pub fn routes() -> Router {
    Router::new()
        .merge(api::code_nav::router())
        .merge(api::fs::router())
        .merge(api::git::router())
        .merge(api::notepad::router())
        .merge(api::projects::router())
}
