use super::*;

pub fn build_internal_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/internal/config/v1/snapshots/{service_name}",
            get(internal_snapshot),
        )
        .route(
            "/internal/config/v1/instances/heartbeat",
            post(instance_heartbeat),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_internal,
        ))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(
            chatos_service_runtime::request_id_middleware,
        ))
}

#[derive(Debug, Deserialize)]
struct SnapshotQuery {
    environment: Option<String>,
}

struct ConfigCenterInternalResourceAudit<'a> {
    resource_type: &'a str,
    resource_id: &'a str,
    resource_name: Option<&'a str>,
    action: &'a str,
    outcome: &'a str,
}

async fn internal_snapshot(
    State(state): State<AppState>,
    Extension(caller): Extension<InternalServiceTokenClaims>,
    Path(service_name): Path<String>,
    Query(query): Query<SnapshotQuery>,
    headers: HeaderMap,
) -> Response {
    let environment = query
        .environment
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(state.config.default_environment.as_str())
        .to_string();
    let resource_id = format!("{environment}/{service_name}");
    let response = load_internal_snapshot(
        &state,
        &caller,
        service_name.as_str(),
        environment.as_str(),
        &headers,
    )
    .await;
    record_config_center_internal_resource_access(
        &caller,
        ConfigCenterInternalResourceAudit {
            resource_type: "config_snapshot",
            resource_id: resource_id.as_str(),
            resource_name: Some(service_name.as_str()),
            action: "read",
            outcome: internal_response_outcome(response.status()),
        },
    );
    response
}

async fn load_internal_snapshot(
    state: &AppState,
    caller: &InternalServiceTokenClaims,
    service_name: &str,
    environment: &str,
    headers: &HeaderMap,
) -> Response {
    if let Err(err) =
        require_matching_service_identity(caller.caller.as_str(), service_name, "snapshot")
    {
        return error(StatusCode::FORBIDDEN, err);
    }
    match state.snapshot(environment, service_name).await {
        Ok(snapshot) => {
            let quoted_etag = snapshot.etag();
            if headers
                .get(IF_NONE_MATCH)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value == quoted_etag)
            {
                return StatusCode::NOT_MODIFIED.into_response();
            }
            let mut response = Json(snapshot).into_response();
            if let Ok(value) = HeaderValue::from_str(quoted_etag.as_str()) {
                response.headers_mut().insert(ETAG, value);
            }
            response
        }
        Err(err) => error(StatusCode::NOT_FOUND, err),
    }
}

async fn instance_heartbeat(
    State(state): State<AppState>,
    Extension(caller): Extension<InternalServiceTokenClaims>,
    Json(input): Json<InstanceHeartbeatRequest>,
) -> Response {
    let resource_id = format!(
        "{}:{}/{}",
        input.environment.trim(),
        input.service_name.trim(),
        input.service_id.trim()
    );
    let resource_name = input.service_id.trim().to_string();
    let response = persist_instance_heartbeat(&state, &caller, input).await;
    record_config_center_internal_resource_access(
        &caller,
        ConfigCenterInternalResourceAudit {
            resource_type: "config_service_instance",
            resource_id: resource_id.as_str(),
            resource_name: Some(resource_name.as_str()),
            action: "heartbeat",
            outcome: internal_response_outcome(response.status()),
        },
    );
    response
}

async fn persist_instance_heartbeat(
    state: &AppState,
    caller: &InternalServiceTokenClaims,
    input: InstanceHeartbeatRequest,
) -> Response {
    if let Err(err) = require_matching_service_identity(
        caller.caller.as_str(),
        input.service_name.as_str(),
        "heartbeat",
    ) {
        return error(StatusCode::FORBIDDEN, err);
    }
    if input
        .pressure
        .as_ref()
        .is_some_and(|signal| signal.reason.trim().is_empty() || signal.reason.trim().len() > 256)
    {
        return error(
            StatusCode::BAD_REQUEST,
            "Pressure signal reason must contain between 1 and 256 bytes",
        );
    }
    let instance = ServiceInstanceRecord {
        id: format!(
            "{}:{}:{}",
            input.environment, input.service_name, input.service_id
        ),
        environment: input.environment,
        service_name: input.service_name,
        service_id: input.service_id,
        running_version: input.running_version,
        effective_revision: input.effective_revision,
        effective_checksum: input.effective_checksum,
        stale: input.stale,
        pending_restart_keys: input.pending_restart_keys,
        emergency_override_keys: input.emergency_override_keys,
        last_error: input.last_error,
        pressure: input.pressure.map(|mut signal| {
            signal.reason = signal.reason.trim().to_string();
            signal
        }),
        last_seen_at: Utc::now().to_rfc3339(),
    };
    result_json(state.heartbeat(instance).await)
}

fn record_config_center_internal_resource_access(
    caller: &InternalServiceTokenClaims,
    access: ConfigCenterInternalResourceAudit<'_>,
) {
    let event = build_config_center_internal_audit_event(caller, access);
    if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
        tracing::error!(
            target: "chatos_internal_audit",
            trace_id = caller.trace_id.as_str(),
            error = error.as_str(),
            "Configuration Center internal resource audit validation failed"
        );
    }
}

fn build_config_center_internal_audit_event(
    caller: &InternalServiceTokenClaims,
    access: ConfigCenterInternalResourceAudit<'_>,
) -> chatos_service_runtime::InternalResourceAccessAudit {
    chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: caller.caller.clone(),
        audience_service: CONFIG_CENTER_AUDIENCE.to_string(),
        scope: caller.scope.clone(),
        trace_id: caller.trace_id.clone(),
        represented_user_id: None,
        tenant_id: None,
        project_id: None,
        resource_type: access.resource_type.to_string(),
        resource_id: access.resource_id.to_string(),
        resource_name: access
            .resource_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        action: access.action.to_string(),
        outcome: access.outcome.to_string(),
    }
}

fn internal_response_outcome(status: StatusCode) -> &'static str {
    if status.is_success() || status == StatusCode::NOT_MODIFIED {
        "accepted"
    } else if status.is_server_error() {
        "failed"
    } else {
        "rejected"
    }
}

async fn require_internal(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let scope = match internal_request_scope(request.method(), request.uri().path()) {
        Some(scope) => scope,
        None => {
            return error(
                StatusCode::FORBIDDEN,
                "Configuration Center internal operation is not allowed",
            )
        }
    };
    let claims = match authenticate_internal_request(
        request.headers(),
        &state.config.caller_signing_secrets,
        scope,
    ) {
        Ok(claims) => claims,
        Err(err) => return error(StatusCode::UNAUTHORIZED, err),
    };
    request.extensions_mut().insert(claims);
    next.run(request).await
}

fn authenticate_internal_request(
    headers: &HeaderMap,
    caller_signing_secrets: &BTreeMap<String, String>,
    scope: &str,
) -> Result<InternalServiceTokenClaims, String> {
    let caller = headers
        .get(CONFIG_CENTER_CALLER_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Missing Configuration Center caller identity".to_string())?;
    let secret = caller_signing_secrets
        .get(caller)
        .ok_or_else(|| "Unknown Configuration Center caller identity".to_string())?;
    let token = headers
        .get(CONFIG_CENTER_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Missing Configuration Center internal token".to_string())?;
    verify_internal_service_token(token, secret, caller, CONFIG_CENTER_AUDIENCE, scope)
}

fn internal_request_scope(method: &Method, path: &str) -> Option<&'static str> {
    if method == Method::GET && path.starts_with("/internal/config/v1/snapshots/") {
        return Some(CONFIG_SNAPSHOT_READ_SCOPE);
    }
    if method == Method::POST && path == "/internal/config/v1/instances/heartbeat" {
        return Some(CONFIG_INSTANCE_HEARTBEAT_SCOPE);
    }
    None
}

fn require_matching_service_identity(
    caller: &str,
    target: &str,
    operation: &str,
) -> Result<(), String> {
    if caller == target {
        return Ok(());
    }
    Err(format!(
        "Configuration {operation} service identity does not match its authenticated caller"
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
    use chatos_service_runtime::{issue_internal_service_token, InternalServiceTokenClaims};
    use uuid::Uuid;

    use super::{
        authenticate_internal_request, build_config_center_internal_audit_event,
        internal_request_scope, internal_response_outcome, require_matching_service_identity,
        ConfigCenterInternalResourceAudit,
    };
    use chatos_config_sdk::{
        CONFIG_CENTER_AUDIENCE, CONFIG_CENTER_CALLER_HEADER, CONFIG_CENTER_TOKEN_HEADER,
        CONFIG_INSTANCE_HEARTBEAT_SCOPE, CONFIG_SNAPSHOT_READ_SCOPE,
    };

    #[test]
    fn internal_routes_have_operation_specific_scopes() {
        assert_eq!(
            internal_request_scope(&Method::GET, "/internal/config/v1/snapshots/task-runner"),
            Some(CONFIG_SNAPSHOT_READ_SCOPE)
        );
        assert_eq!(
            internal_request_scope(&Method::POST, "/internal/config/v1/instances/heartbeat"),
            Some(CONFIG_INSTANCE_HEARTBEAT_SCOPE)
        );
        assert_eq!(
            internal_request_scope(&Method::POST, "/internal/config/v1/snapshots/task-runner"),
            None
        );
    }

    #[test]
    fn caller_keys_are_isolated_and_legacy_static_headers_are_rejected() {
        let secrets = BTreeMap::from([
            (
                "task-runner".to_string(),
                "task-runner-config-center-test-secret".to_string(),
            ),
            (
                "chatos-backend".to_string(),
                "chatos-config-center-test-secret".to_string(),
            ),
        ]);
        let token = issue_internal_service_token(
            secrets["task-runner"].as_str(),
            "task-runner",
            CONFIG_CENTER_AUDIENCE,
            CONFIG_SNAPSHOT_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            CONFIG_CENTER_CALLER_HEADER,
            HeaderValue::from_static("task-runner"),
        );
        headers.insert(
            CONFIG_CENTER_TOKEN_HEADER,
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );
        let claims = authenticate_internal_request(&headers, &secrets, CONFIG_SNAPSHOT_READ_SCOPE)
            .expect("authenticate task runner");
        assert_eq!(claims.caller, "task-runner");

        headers.insert(
            CONFIG_CENTER_CALLER_HEADER,
            HeaderValue::from_static("chatos-backend"),
        );
        assert!(
            authenticate_internal_request(&headers, &secrets, CONFIG_SNAPSHOT_READ_SCOPE).is_err()
        );

        let mut legacy_headers = HeaderMap::new();
        legacy_headers.insert(
            "x-config-center-internal-secret",
            HeaderValue::from_static("task-runner-config-center-test-secret"),
        );
        assert!(authenticate_internal_request(
            &legacy_headers,
            &secrets,
            CONFIG_SNAPSHOT_READ_SCOPE
        )
        .is_err());
    }

    #[test]
    fn operation_scope_cannot_be_reused() {
        let secrets = BTreeMap::from([(
            "task-runner".to_string(),
            "task-runner-config-center-test-secret".to_string(),
        )]);
        let token = issue_internal_service_token(
            secrets["task-runner"].as_str(),
            "task-runner",
            CONFIG_CENTER_AUDIENCE,
            CONFIG_SNAPSHOT_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            CONFIG_CENTER_CALLER_HEADER,
            HeaderValue::from_static("task-runner"),
        );
        headers.insert(
            CONFIG_CENTER_TOKEN_HEADER,
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );
        assert!(
            authenticate_internal_request(&headers, &secrets, CONFIG_INSTANCE_HEARTBEAT_SCOPE)
                .is_err()
        );
    }

    #[test]
    fn authenticated_caller_must_match_snapshot_path_and_heartbeat_body() {
        assert!(
            require_matching_service_identity("task-runner", "task-runner", "snapshot").is_ok()
        );
        assert!(
            require_matching_service_identity("task-runner", "chatos-backend", "snapshot").is_err()
        );
        assert!(
            require_matching_service_identity("task-runner", "chatos-backend", "heartbeat")
                .is_err()
        );
    }

    #[test]
    fn internal_audit_uses_verified_trace_scope_and_resource_identity() {
        let claims = InternalServiceTokenClaims {
            iss: "task-runner".to_string(),
            sub: "task-runner".to_string(),
            caller: "task-runner".to_string(),
            aud: CONFIG_CENTER_AUDIENCE.to_string(),
            scope: CONFIG_SNAPSHOT_READ_SCOPE.to_string(),
            trace_id: Uuid::new_v4().to_string(),
            iat: 1,
            exp: 2,
        };
        let event = build_config_center_internal_audit_event(
            &claims,
            ConfigCenterInternalResourceAudit {
                resource_type: "config_snapshot",
                resource_id: "local/task-runner",
                resource_name: Some("task-runner"),
                action: "read",
                outcome: "accepted",
            },
        );

        assert!(event.validate().is_ok());
        assert_eq!(event.trace_id, claims.trace_id);
        assert_eq!(event.scope, CONFIG_SNAPSHOT_READ_SCOPE);
        assert_eq!(event.resource_id, "local/task-runner");
    }

    #[test]
    fn not_modified_snapshot_is_an_accepted_internal_access() {
        assert_eq!(internal_response_outcome(StatusCode::OK), "accepted");
        assert_eq!(
            internal_response_outcome(StatusCode::NOT_MODIFIED),
            "accepted"
        );
        assert_eq!(
            internal_response_outcome(StatusCode::BAD_REQUEST),
            "rejected"
        );
        assert_eq!(
            internal_response_outcome(StatusCode::INTERNAL_SERVER_ERROR),
            "failed"
        );
    }
}
