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

pub(in crate::api) const CHATOS_CALLER: &str = "chatos-backend";
pub(in crate::api) const TASK_RUNNER_CALLER: &str = "task-runner";
pub(in crate::api) const PROJECT_SERVICE_CALLER: &str = "project-service";

pub(in crate::api) fn require_project_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    allowed_callers: &[&str],
    required_scope: &str,
) -> Result<(), ApiError> {
    let caller = header_text(headers, "x-project-service-caller");
    let token = header_text(headers, "x-project-service-internal-token");

    if let Some(caller) = caller {
        if !allowed_callers.contains(&caller) {
            return Err(ApiError::forbidden(
                "caller service is not allowed for this project service operation",
            ));
        }
        let expected = config
            .internal_api_secrets
            .get(caller)
            .map(String::as_str)
            .or_else(|| {
                (!config.require_signed_internal_requests)
                    .then_some(config.sync_secret.as_deref())
                    .flatten()
            })
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::unauthorized("project service internal API is disabled"))?;
        if let Some(token) = token {
            chatos_service_runtime::verify_internal_service_token(
                token,
                expected,
                caller,
                PROJECT_SERVICE_TOKEN_AUDIENCE,
                required_scope,
            )
            .map_err(|_| ApiError::unauthorized("invalid project service internal API token"))?;
            return Ok(());
        }
        if config.require_signed_internal_requests {
            return Err(ApiError::unauthorized(
                "signed project service internal API token is required",
            ));
        }
        require_legacy_secret(headers, expected)?;
        return Ok(());
    }

    if token.is_some() {
        return Err(ApiError::bad_request(
            "project service caller is required for signed internal requests",
        ));
    }
    if config.require_signed_internal_requests {
        return Err(ApiError::unauthorized(
            "signed project service internal API token is required",
        ));
    }
    let expected = config
        .sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::forbidden("project sync secret is not configured"))?;
    require_legacy_secret(headers, expected)?;
    Ok(())
}

fn require_legacy_secret(headers: &HeaderMap, expected: &str) -> Result<(), ApiError> {
    let provided = header_text(headers, "x-project-service-sync-secret")
        .or_else(|| header_text(headers, "x-chatos-callback-secret"))
        .ok_or_else(|| ApiError::unauthorized("missing project sync secret"))?;
    if !constant_time_eq(expected.as_bytes(), provided.as_bytes()) {
        return Err(ApiError::unauthorized("invalid project sync secret"));
    }
    Ok(())
}

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let mut difference = expected.len() ^ actual.len();
    for (left, right) in expected.iter().zip(actual.iter()) {
        difference |= usize::from(left ^ right);
    }
    difference == 0
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

        require_project_internal_request(
            &config,
            &headers,
            &[TASK_RUNNER_CALLER],
            PROJECT_READ_SCOPE,
        )
        .expect("matching signed request");
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
            "x-project-service-sync-secret",
            HeaderValue::from_static("chatos-dedicated-secret"),
        );

        let err = require_project_internal_request(
            &config,
            &headers,
            &[TASK_RUNNER_CALLER],
            PROJECT_READ_SCOPE,
        )
        .expect_err("another caller secret must fail");
        assert_eq!(err.message, "invalid project sync secret");
    }

    #[test]
    fn production_style_config_rejects_legacy_only_request() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        let mut headers = HeaderMap::new();
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
            database_url:
                "mongodb://admin:admin@127.0.0.1:27018/project_management_test?authSource=admin"
                    .to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_service_request_timeout: Duration::from_secs(1),
            memory_engine_base_url: "http://127.0.0.1:7081".to_string(),
            memory_engine_source_id: "test".to_string(),
            memory_engine_operator_token: None,
            memory_engine_request_timeout: Duration::from_secs(1),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            sandbox_image_mcp_request_timeout: Duration::from_secs(1),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024,
            cloud_project_max_unpacked_bytes: 1024,
            cloud_project_max_files: 10,
            cloud_project_git_timeout: Duration::from_secs(1),
            task_runner_base_url: None,
            task_runner_request_timeout: Duration::from_secs(1),
            task_runner_internal_secret: None,
            sync_secret: Some("legacy-sync-secret".to_string()),
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
        }
    }
}
