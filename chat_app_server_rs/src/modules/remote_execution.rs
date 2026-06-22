use axum::Router;

use crate::api;

pub fn routes() -> Router {
    Router::new()
        .merge(api::remote_connections::router())
        .merge(api::terminals::router())
}
