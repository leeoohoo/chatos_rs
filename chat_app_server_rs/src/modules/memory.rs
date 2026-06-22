use axum::Router;

use crate::api;

pub fn routes() -> Router {
    Router::new()
        .merge(api::memory_compat::router())
        .merge(api::memory_mappings::router())
}
