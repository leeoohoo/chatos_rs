// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::HeaderMap;

use super::ApiError;
use crate::config::AppConfig;

pub(in crate::api) const PROJECT_SERVICE_TOKEN_AUDIENCE: &str = "project-service";
pub(in crate::api) const PROJECT_READ_SCOPE: &str = "project.read";
pub(in crate::api) const PROJECT_SYNC_SCOPE: &str = "project.sync";
pub(in crate::api) const PROJECT_MCP_SCOPE: &str = "project.mcp";
pub(in crate::api) const PROJECT_HARNESS_SCOPE: &str = "project.harness";
pub(in crate::api) const PROJECT_ENVIRONMENT_SCOPE: &str = "project.environment";
pub(in crate::api) const PROJECT_EXECUTION_CONTEXT_SCOPE: &str = "project.execution_context.read";

pub(in crate::api) const CHATOS_CALLER: &str = "chatos-backend";
pub(in crate::api) const TASK_RUNNER_CALLER: &str = "task-runner";
pub(in crate::api) const PROJECT_SERVICE_CALLER: &str = "project-service";
pub(in crate::api) const MCP_MANAGEMENT_CALLER: &str = "mcp-management-service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::api) struct ProjectInternalRequestIdentity {
    pub caller_service: String,
    pub scope: String,
    pub trace_id: String,
}

pub(in crate::api) struct ProjectInternalResourceAudit<'a> {
    pub represented_user_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub resource_type: &'a str,
    pub resource_id: &'a str,
    pub resource_name: Option<&'a str>,
    pub action: &'a str,
    pub outcome: &'a str,
}

pub(in crate::api) fn record_project_internal_resource_access(
    identity: &ProjectInternalRequestIdentity,
    access: ProjectInternalResourceAudit<'_>,
) {
    let event = chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: identity.caller_service.clone(),
        audience_service: PROJECT_SERVICE_TOKEN_AUDIENCE.to_string(),
        scope: identity.scope.clone(),
        trace_id: identity.trace_id.clone(),
        represented_user_id: normalized_optional(access.represented_user_id),
        tenant_id: None,
        project_id: normalized_optional(access.project_id),
        resource_type: access.resource_type.to_string(),
        resource_id: access.resource_id.to_string(),
        resource_name: normalized_optional(access.resource_name),
        action: access.action.to_string(),
        outcome: access.outcome.to_string(),
    };
    if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
        tracing::error!(
            target: "chatos_internal_audit",
            trace_id = identity.trace_id.as_str(),
            error = error.as_str(),
            "project internal resource audit validation failed"
        );
    }
}

pub(in crate::api) fn require_project_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    allowed_callers: &[&str],
    required_scope: &str,
) -> Result<ProjectInternalRequestIdentity, ApiError> {
    let token = header_text(headers, "x-project-service-internal-token");
    let caller = match header_text(headers, "x-project-service-caller") {
        Some(caller) => caller,
        None if token.is_some() => {
            return Err(ApiError::bad_request(
                "project service caller is required for signed internal requests",
            ));
        }
        None => {
            return Err(ApiError::unauthorized(
                "signed project service internal API token is required",
            ));
        }
    };
    if !allowed_callers.contains(&caller) {
        return Err(ApiError::forbidden(
            "caller service is not allowed for this project service operation",
        ));
    }
    let expected = config
        .internal_api_secrets
        .get(caller)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("project service internal API is disabled"))?;
    let token = token.ok_or_else(|| {
        ApiError::unauthorized("signed project service internal API token is required")
    })?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token,
        expected,
        caller,
        PROJECT_SERVICE_TOKEN_AUDIENCE,
        required_scope,
    )
    .map_err(|_| ApiError::unauthorized("invalid project service internal API token"))?;
    Ok(ProjectInternalRequestIdentity {
        caller_service: caller.to_string(),
        scope: required_scope.to_string(),
        trace_id: claims.trace_id,
    })
}

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn signed_token_binds_caller_audience_and_scope() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config.internal_api_secrets.insert(
            TASK_RUNNER_CALLER.to_string(),
            "a-long-task-runner-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-secret",
            TASK_RUNNER_CALLER,
            PROJECT_SERVICE_TOKEN_AUDIENCE,
            PROJECT_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-caller",
            HeaderValue::from_static(TASK_RUNNER_CALLER),
        );
        headers.insert(
            "x-project-service-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );

        let identity = require_project_internal_request(
            &config,
            &headers,
            &[TASK_RUNNER_CALLER],
            PROJECT_READ_SCOPE,
        )
        .expect("matching signed request");
        assert_eq!(identity.caller_service, TASK_RUNNER_CALLER);
        assert_eq!(identity.scope, PROJECT_READ_SCOPE);
        uuid::Uuid::parse_str(identity.trace_id.as_str()).expect("signed trace id");
        let err = require_project_internal_request(
            &config,
            &headers,
            &[TASK_RUNNER_CALLER],
            PROJECT_SYNC_SCOPE,
        )
        .expect_err("scope mismatch must fail");
        assert_eq!(err.message, "invalid project service internal API token");
    }

    #[test]
    fn caller_secret_cannot_impersonate_another_service() {
        let mut config = test_config();
        config.internal_api_secrets.insert(
            CHATOS_CALLER.to_string(),
            "chatos-dedicated-secret".to_string(),
        );
        config.internal_api_secrets.insert(
            TASK_RUNNER_CALLER.to_string(),
            "task-runner-dedicated-secret".to_string(),
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-caller",
            HeaderValue::from_static(TASK_RUNNER_CALLER),
        );
        headers.insert(
            "x-project-service-internal-token",
            HeaderValue::from_str(
                chatos_service_runtime::issue_internal_service_token(
                    "chatos-dedicated-secret",
                    TASK_RUNNER_CALLER,
                    PROJECT_SERVICE_TOKEN_AUDIENCE,
                    PROJECT_READ_SCOPE,
                    60,
                )
                .expect("issue impersonation token")
                .as_str(),
            )
            .expect("token header"),
        );

        let err = require_project_internal_request(
            &config,
            &headers,
            &[TASK_RUNNER_CALLER],
            PROJECT_READ_SCOPE,
        )
        .expect_err("another caller secret must fail");
        assert_eq!(err.message, "invalid project service internal API token");
    }

    #[test]
    fn legacy_only_request_is_rejected_without_a_signed_trace() {
        let mut config = test_config();
        config.internal_api_secrets.insert(
            CHATOS_CALLER.to_string(),
            "chatos-dedicated-secret".to_string(),
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-caller",
            HeaderValue::from_static(CHATOS_CALLER),
        );
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("legacy-sync-secret"),
        );

        let err = require_project_internal_request(
            &config,
            &headers,
            &[CHATOS_CALLER, TASK_RUNNER_CALLER],
            PROJECT_READ_SCOPE,
        )
        .expect_err("legacy-only auth must fail");
        assert_eq!(
            err.message,
            "signed project service internal API token is required"
        );
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            database_url:
                "mongodb://admin:admin@127.0.0.1:27018/project_management_test?authSource=admin"
                    .to_string(),
            mcp_result_rabbitmq_url: "amqp://127.0.0.1:1/%2f".to_string(),
            mcp_result_queue_prefix: "project_service.mcp.results.test".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_internal_base_url: "https://127.0.0.1:39192".to_string(),
            user_service_internal_http_client: reqwest::Client::new(),
            user_service_request_timeout: Duration::from_secs(1),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_http_client: reqwest::Client::new(),
            local_connector_service_request_timeout: Duration::from_secs(1),
            memory_engine_base_url: "http://127.0.0.1:7081".to_string(),
            memory_engine_http_client: reqwest::Client::new(),
            memory_engine_source_id: "test".to_string(),
            memory_engine_internal_api_secret: None,
            memory_engine_request_timeout: Duration::from_secs(1),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_http_client: reqwest::Client::new(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            sandbox_image_mcp_request_timeout: Duration::from_secs(1),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024,
            cloud_project_max_unpacked_bytes: 1024,
            cloud_project_max_files: 10,
            cloud_project_git_timeout: Duration::from_secs(1),
            environment_analysis_timeout: Duration::from_secs(60),
            environment_analysis_stale_after: Duration::from_secs(60),
            task_runner_base_url: None,
            task_runner_request_timeout: Duration::from_secs(1),
            task_runner_internal_secret: None,
            sync_secret: Some("legacy-sync-secret".to_string()),
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
        }
    }
}
