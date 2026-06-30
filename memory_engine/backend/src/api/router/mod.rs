use std::sync::Arc;

use axum::{middleware, Router};

use crate::api::{memory_auth, model_profile_auth, operator_auth};
use crate::state::AppState;

mod admin;
mod core;
mod sdk;

pub fn build_router(state: Arc<AppState>) -> Router {
    let protected_state = state.clone();

    Router::new()
        .merge(
            admin::model_profile_routes().route_layer(middleware::from_fn_with_state(
                protected_state.clone(),
                model_profile_auth::require_model_profile_auth,
            )),
        )
        .merge(admin::routes().route_layer(middleware::from_fn_with_state(
            protected_state.clone(),
            memory_auth::require_memory_auth,
        )))
        .merge(sdk::routes())
        .merge(core::public_routes())
        .merge(
            core::operator_routes().route_layer(middleware::from_fn_with_state(
                protected_state.clone(),
                operator_auth::require_operator_auth,
            )),
        )
        .merge(
            core::data_routes().route_layer(middleware::from_fn_with_state(
                protected_state,
                memory_auth::require_memory_auth,
            )),
        )
        .with_state(state)
}
