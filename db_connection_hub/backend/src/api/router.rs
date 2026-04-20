use crate::{api::handlers, state::AppState};
use axum::{
    http::Method,
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};

pub fn build_router(app_state: AppState) -> Router {
    // Keep local development smooth: frontend and backend run on different localhost ports.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    Router::new()
        .route("/api/v1/health", get(handlers::health::health_check))
        .route("/api/v1/meta/db-types", get(handlers::meta::list_db_types))
        .route(
            "/api/v1/meta/discover-databases",
            post(handlers::meta::discover_databases),
        )
        .route(
            "/api/v1/meta/test-connection",
            post(handlers::meta::test_connection),
        )
        .route(
            "/api/v1/datasources",
            get(handlers::datasources::list_datasources)
                .post(handlers::datasources::create_datasource),
        )
        .route(
            "/api/v1/datasources/:id",
            get(handlers::datasources::get_datasource)
                .put(handlers::datasources::update_datasource)
                .delete(handlers::datasources::delete_datasource),
        )
        .route(
            "/api/v1/datasources/:id/test",
            post(handlers::datasources::test_datasource),
        )
        .route(
            "/api/v1/datasources/:id/health",
            get(handlers::datasources::datasource_health),
        )
        .route(
            "/api/v1/datasources/:id/databases/summary",
            get(handlers::datasources::database_summary),
        )
        .route(
            "/api/v1/datasources/:id/databases",
            get(handlers::datasources::list_databases),
        )
        .route(
            "/api/v1/datasources/:id/databases/:database/object-stats",
            get(handlers::datasources::object_stats),
        )
        .route(
            "/api/v1/metadata/nodes",
            get(handlers::metadata::list_nodes),
        )
        .route(
            "/api/v1/metadata/object-detail",
            get(handlers::metadata::object_detail),
        )
        .route(
            "/api/v1/queries/execute",
            post(handlers::queries::execute_query),
        )
        .route(
            "/api/v1/queries/:id/cancel",
            post(handlers::queries::cancel_query),
        )
        .layer(cors)
        .with_state(app_state)
}
