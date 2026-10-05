// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt::Write;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::models::{HealthResponse, SystemConfigResponse, SystemDatabaseStatus};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::ApiResult;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "user_service_backend".to_string(),
        now: now_rfc3339(),
    })
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.retention.stats();
    let mut body = String::new();
    for (name, help, metric_type, value) in [
        (
            "chatos_user_service_retention_successful_runs_total",
            "Successful User Service retention runs.",
            "counter",
            stats.successful_runs_total,
        ),
        (
            "chatos_user_service_retention_failed_runs_total",
            "Failed User Service retention runs.",
            "counter",
            stats.failed_runs_total,
        ),
        (
            "chatos_user_service_retention_deleted_rows_total",
            "Expired User Service rows deleted by retention.",
            "counter",
            stats.deleted_rows_total,
        ),
        (
            "chatos_user_service_retention_last_success_unix",
            "Unix timestamp of the last successful User Service retention run; zero means none.",
            "gauge",
            stats.last_success_unix.unwrap_or_default() as u64,
        ),
    ] {
        let _ = writeln!(body, "# HELP {name} {help}");
        let _ = writeln!(body, "# TYPE {name} {metric_type}");
        let _ = writeln!(body, "{name}{{service=\"user-service\"}} {value}");
    }
    body.push_str(&chatos_postgres::render_pool_metrics(
        state.store.pool(),
        "user-service",
    ));
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

pub async fn get_system_config(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<SystemConfigResponse> {
    super::require_super_admin(&principal)?;
    Ok(Json(SystemConfigResponse {
        service: "user_service_backend".to_string(),
        issuer: state.config.jwt_issuer.clone(),
        user_service_audience: state.config.user_service_audience.clone(),
        database: SystemDatabaseStatus {
            kind: "postgresql".to_string(),
            configured: !state.config.database_url.trim().is_empty(),
        },
        user_access_ttl_seconds: state.config.user_access_ttl_seconds,
    }))
}

#[cfg(test)]
mod tests {
    use crate::auth::CurrentPrincipal;
    use crate::models::{
        SystemConfigResponse, SystemDatabaseStatus, PRINCIPAL_TYPE_AGENT_ACCOUNT,
        PRINCIPAL_TYPE_HUMAN_USER, USER_ROLE_SUPER_ADMIN, USER_ROLE_USER,
    };

    fn principal(principal_type: &str, role: Option<&str>) -> CurrentPrincipal {
        CurrentPrincipal {
            sub: "test".to_string(),
            jti: "test-jti".to_string(),
            exp: usize::MAX,
            principal_type: principal_type.to_string(),
            user_id: Some("user-1".to_string()),
            username: Some("test@example.com".to_string()),
            display_name: Some("Test".to_string()),
            role: role.map(str::to_string),
            credential_version: Some(0),
            agent_account_id: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            scopes: Vec::new(),
        }
    }

    #[test]
    fn system_config_rejects_ordinary_and_agent_principals() {
        assert!(super::super::require_super_admin(&principal(
            PRINCIPAL_TYPE_HUMAN_USER,
            Some(USER_ROLE_USER)
        ))
        .is_err());
        assert!(super::super::require_super_admin(&principal(
            PRINCIPAL_TYPE_AGENT_ACCOUNT,
            Some(USER_ROLE_SUPER_ADMIN)
        ))
        .is_err());
        assert!(super::super::require_super_admin(&principal(
            PRINCIPAL_TYPE_HUMAN_USER,
            Some(USER_ROLE_SUPER_ADMIN)
        ))
        .is_ok());
    }

    #[test]
    fn system_config_response_cannot_serialize_database_credentials() {
        let response = SystemConfigResponse {
            service: "user_service_backend".to_string(),
            issuer: "user_service".to_string(),
            user_service_audience: "user_service".to_string(),
            database: SystemDatabaseStatus {
                kind: "postgresql".to_string(),
                configured: true,
            },
            user_access_ttl_seconds: 3600,
        };
        let serialized = serde_json::to_string(&response).expect("serialize system config");
        assert!(!serialized.contains("database_url"));
        assert!(!serialized.contains("postgresql://"));
        assert!(!serialized.contains("password"));
    }
}
