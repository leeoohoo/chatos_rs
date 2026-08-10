use super::*;

pub fn build_public_router(state: AppState) -> Router {
    let cors = if state.config.cors_origins.iter().any(|origin| origin == "*") {
        CorsLayer::new().allow_origin(Any)
    } else {
        let origins = state
            .config
            .cors_origins
            .iter()
            .filter_map(|origin| match HeaderValue::from_str(origin) {
                Ok(value) => Some(value),
                Err(err) => {
                    tracing::warn!(origin, error = %err, "ignoring invalid CORS origin");
                    None
                }
            })
            .collect::<Vec<_>>();
        CorsLayer::new().allow_origin(AllowOrigin::list(origins))
    }
    .allow_headers(Any)
    .allow_methods(Any);

    let admin = Router::new()
        .route("/api/auth/me", get(me))
        .route("/api/config/v1/catalog", get(catalog))
        .route(
            "/api/config/v1/catalog/custom",
            post(create_custom_definition),
        )
        .route(
            "/api/config/v1/environments/{environment}/effective",
            get(effective),
        )
        .route(
            "/api/config/v1/environments/{environment}/draft",
            get(get_draft).put(update_draft),
        )
        .route(
            "/api/config/v1/environments/{environment}/draft/validate",
            post(validate_draft),
        )
        .route(
            "/api/config/v1/environments/{environment}/draft/publish",
            post(publish_draft),
        )
        .route(
            "/api/config/v1/environments/{environment}/releases",
            get(releases),
        )
        .route(
            "/api/config/v1/environments/{environment}/releases/{release_id}/rollback",
            post(rollback),
        )
        .route("/api/config/v1/audit-events", get(audit_events))
        .route(
            "/api/config/v1/environments/{environment}/queue-operations",
            get(queue_operations).post(replay_queue_operation),
        )
        .route(
            "/api/config/v1/environments/{environment}/pressure",
            get(pressure_status),
        )
        .route("/api/config/v1/instances", get(instances))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_admin));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/auth/login", post(login))
        .merge(admin)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "configuration-center".to_string(),
    })
}

async fn ready(State(state): State<AppState>) -> Response {
    match state.store.ping().await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(err) => error(StatusCode::SERVICE_UNAVAILABLE, err),
    }
}

async fn login(State(state): State<AppState>, Json(input): Json<LoginRequest>) -> Response {
    match auth::login(&state.config, state.http_client(), &input).await {
        Ok(response) if response.user.is_super_admin() => Json(response).into_response(),
        Ok(_) => error(
            StatusCode::FORBIDDEN,
            "Configuration center requires super_admin access",
        ),
        Err(err) => error(StatusCode::UNAUTHORIZED, err),
    }
}

async fn me(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}

async fn catalog(State(state): State<AppState>) -> Response {
    result_json(state.store.list_definitions().await)
}

async fn create_custom_definition(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CustomDefinitionRequest>,
) -> Response {
    result_json(state.create_custom_definition(input, &user).await)
}

async fn effective(State(state): State<AppState>, Path(environment): Path<String>) -> Response {
    result_json(state.effective(environment.as_str()).await)
}

async fn get_draft(State(state): State<AppState>, Path(environment): Path<String>) -> Response {
    match state.store.get_draft(environment.as_str()).await {
        Ok(draft) => {
            let active_revision = state
                .store
                .get_active(environment.as_str())
                .await
                .ok()
                .flatten()
                .map(|active| active.revision)
                .unwrap_or_default();
            Json(json!({
                "environment": environment,
                "active_revision": active_revision,
                "draft": draft,
            }))
            .into_response()
        }
        Err(err) => error(StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

async fn update_draft(
    State(state): State<AppState>,
    Path(environment): Path<String>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<DraftUpdateRequest>,
) -> Response {
    result_json(
        state
            .save_draft(environment.as_str(), input.changes, &user)
            .await,
    )
}

async fn validate_draft(
    State(state): State<AppState>,
    Path(environment): Path<String>,
) -> Response {
    result_json(state.validate_draft(environment.as_str()).await)
}

async fn publish_draft(
    State(state): State<AppState>,
    Path(environment): Path<String>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<PublishRequest>,
) -> Response {
    let message = input
        .message
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Publish configuration changes");
    result_json(
        state
            .publish_draft(environment.as_str(), &user, message)
            .await,
    )
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

async fn releases(
    State(state): State<AppState>,
    Path(environment): Path<String>,
    Query(query): Query<LimitQuery>,
) -> Response {
    result_json(
        state
            .store
            .list_releases(
                environment.as_str(),
                query.limit.unwrap_or(100).clamp(1, 500),
            )
            .await,
    )
}

async fn rollback(
    State(state): State<AppState>,
    Path((environment, release_id)): Path<(String, String)>,
    Extension(user): Extension<CurrentUser>,
) -> Response {
    result_json(
        state
            .rollback(environment.as_str(), release_id.as_str(), &user)
            .await,
    )
}

async fn audit_events(State(state): State<AppState>, Query(query): Query<LimitQuery>) -> Response {
    result_json(
        state
            .store
            .list_audit(query.limit.unwrap_or(200).clamp(1, 1000))
            .await,
    )
}

async fn queue_operations(
    State(state): State<AppState>,
    Path(environment): Path<String>,
) -> Response {
    result_json(crate::queue_operations::inspect(&state, environment.as_str()).await)
}

async fn pressure_status(
    State(state): State<AppState>,
    Path(environment): Path<String>,
) -> Response {
    result_json(crate::state::pressure_controller::status(&state, environment.as_str()).await)
}

async fn replay_queue_operation(
    State(state): State<AppState>,
    Path(environment): Path<String>,
    Extension(user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(input): Json<crate::queue_operations::QueueReplayRequest>,
) -> Response {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    result_json(
        crate::queue_operations::replay(&state, environment.as_str(), &user, authorization, input)
            .await,
    )
}

async fn instances(State(state): State<AppState>) -> Response {
    result_json(state.store.list_instances().await)
}

async fn require_admin(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let token = match auth::bearer_token(request.headers()) {
        Ok(token) => token,
        Err(err) => return error(StatusCode::UNAUTHORIZED, err),
    };
    match auth::verify(&state.config, state.http_client(), token).await {
        Ok(user) if user.is_super_admin() => {
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Ok(_) => error(
            StatusCode::FORBIDDEN,
            "Configuration center requires super_admin access",
        ),
        Err(err) => error(StatusCode::UNAUTHORIZED, err),
    }
}
