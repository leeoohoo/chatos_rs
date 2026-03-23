mod ai_handlers;
mod context_handlers;
mod contracts;
mod support;

use axum::{
    routing::{get, post, put},
    Router,
};

use self::ai_handlers::{
    evaluate_system_context_draft, generate_system_context_draft, optimize_system_context_draft,
};
use self::context_handlers::{
    activate_system_context, create_system_context, delete_system_context,
    get_active_system_context, list_system_contexts, update_system_context,
};

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/system-contexts",
            get(list_system_contexts).post(create_system_context),
        )
        .route(
            "/api/system-contexts/:context_id",
            put(update_system_context).delete(delete_system_context),
        )
        .route(
            "/api/system-contexts/:context_id/activate",
            post(activate_system_context),
        )
        .route(
            "/api/system-contexts/ai/generate",
            post(generate_system_context_draft),
        )
        .route(
            "/api/system-contexts/ai/optimize",
            post(optimize_system_context_draft),
        )
        .route(
            "/api/system-contexts/ai/evaluate",
            post(evaluate_system_context_draft),
        )
        .route("/api/system-context/active", get(get_active_system_context))
}
