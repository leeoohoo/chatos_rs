use axum::Router;

use super::{conversation_runtime, memory, platform_admin, remote_execution, workspace};

pub fn public_routes() -> Router {
    Router::new()
        .merge(platform_admin::public_routes())
        .merge(conversation_runtime::public_routes())
}

pub fn protected_routes() -> Router {
    Router::new()
        .merge(conversation_runtime::routes())
        .merge(memory::routes())
        .merge(platform_admin::protected_routes())
        .merge(remote_execution::routes())
        .merge(workspace::routes())
}
