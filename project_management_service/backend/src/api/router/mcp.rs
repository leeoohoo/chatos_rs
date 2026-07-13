// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::Value;

use crate::api::internal_auth::{
    require_project_internal_request, CHATOS_CALLER, PROJECT_MCP_SCOPE, TASK_RUNNER_CALLER,
};
use crate::api::ApiError;
use crate::auth::{bearer_token_from_headers, verify_token_via_user_service, CurrentUser};
use crate::mcp_server::{self, JsonRpcRequest, JsonRpcResponse, McpServerInfo};
use crate::models::UserRole;
use crate::state::AppState;

pub(super) async fn get_mcp_server_info() -> Json<McpServerInfo> {
    Json(mcp_server::server_info())
}

pub(super) async fn list_mcp_tools() -> Json<Vec<Value>> {
    Json(mcp_server::tool_definitions())
}

pub(super) async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let real_user_access_token = match user_access_token_from_headers(&headers) {
        Ok(value) => value,
        Err(message) => {
            return Json(mcp_server::jsonrpc_error_response(
                StatusCode::UNAUTHORIZED,
                id,
                message,
            ));
        }
    };
    let current_user = match task_runner_internal_mcp_user(&state.config, &headers) {
        Ok(Some(user)) => user,
        Ok(None) => {
            let token = match bearer_token_from_headers(&headers) {
                Ok(token) => token.to_string(),
                Err(message) => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        message,
                    ));
                }
            };
            let current_user = match verify_token_via_user_service(&state.config, &token).await {
                Ok(user) => user,
                Err(message) => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        message,
                    ));
                }
            };
            if !current_user.is_agent_account() {
                return Json(mcp_server::jsonrpc_error_response(
                    StatusCode::UNAUTHORIZED,
                    id,
                    "project management MCP requires an agent account token".to_string(),
                ));
            }
            let user_access_token = match real_user_access_token.as_deref() {
                Some(value) => value,
                None => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        "project management MCP requires a real user token header".to_string(),
                    ));
                }
            };
            let user = match verify_token_via_user_service(&state.config, user_access_token).await {
                Ok(user) => user,
                Err(message) => {
                    return Json(mcp_server::jsonrpc_error_response(
                        StatusCode::UNAUTHORIZED,
                        id,
                        message,
                    ));
                }
            };
            if !user.is_human_user() {
                return Json(mcp_server::jsonrpc_error_response(
                    StatusCode::UNAUTHORIZED,
                    id,
                    "project management MCP real user token must belong to a human user"
                        .to_string(),
                ));
            }
            if let Err(message) = ensure_same_owner_scope(&current_user, &user) {
                return Json(mcp_server::jsonrpc_error_response(
                    StatusCode::FORBIDDEN,
                    id,
                    message,
                ));
            }
            current_user.with_owner_identity_from(&user)
        }
        Err(err) => {
            return Json(mcp_server::jsonrpc_error_response(
                err.status,
                id,
                err.message,
            ));
        }
    };
    let project_id = project_id_from_headers(&headers);
    Json(mcp_server::handle_jsonrpc(state, current_user, project_id, request).await)
}

fn project_id_from_headers(headers: &HeaderMap) -> Option<String> {
    header_text(headers, "x-chatos-project-id")
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn user_access_token_from_headers(headers: &HeaderMap) -> Result<Option<String>, String> {
    for key in [
        "x-chatos-user-authorization",
        "x-user-service-authorization",
        "x-chatos-user-token",
    ] {
        let Some(value) = header_text(headers, key)? else {
            continue;
        };
        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::trim)
            .unwrap_or(value.as_str());
        if !token.is_empty() {
            return Ok(Some(token.to_string()));
        }
    }
    Ok(None)
}

fn ensure_same_owner_scope(agent_user: &CurrentUser, user: &CurrentUser) -> Result<(), String> {
    let agent_owner = agent_user
        .effective_owner_user_id()
        .ok_or_else(|| "agent token missing owner scope".to_string())?;
    let user_owner = user
        .effective_owner_user_id()
        .ok_or_else(|| "user token missing owner scope".to_string())?;
    if agent_owner == user_owner {
        Ok(())
    } else {
        Err("agent token and user token owner scope do not match".to_string())
    }
}

fn task_runner_internal_mcp_user(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
) -> Result<Option<CurrentUser>, ApiError> {
    let has_internal_auth = [
        "x-project-service-caller",
        "x-project-service-internal-token",
        "x-project-service-sync-secret",
    ]
    .into_iter()
    .any(|key| headers.contains_key(key));
    if !has_internal_auth {
        return Ok(None);
    }
    require_project_internal_request(
        config,
        headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_MCP_SCOPE,
    )?;
    let task_profile = header_text(headers, "x-task-runner-task-profile")
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::forbidden("task runner MCP sync branch requires task profile"))?;
    if !is_supported_task_runner_mcp_profile(task_profile.as_str()) {
        return Err(ApiError::forbidden(
            "task runner MCP sync branch only supports chatos_plan",
        ));
    }
    let owner_user_id = header_text(headers, "x-task-runner-owner-user-id")
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::unauthorized("task runner MCP missing owner user id"))?;
    let owner_username = header_text(headers, "x-task-runner-owner-username")
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| owner_user_id.clone());
    let owner_display_name = header_text(headers, "x-task-runner-owner-display-name")
        .map_err(ApiError::bad_request)?
        .or_else(|| Some(owner_username.clone()))
        .unwrap_or_else(|| owner_user_id.clone());
    Ok(Some(CurrentUser {
        principal_type: "human_user".to_string(),
        id: owner_user_id.clone(),
        username: owner_username.clone(),
        display_name: owner_display_name.clone(),
        role: UserRole::Agent,
        owner_user_id: Some(owner_user_id),
        owner_username: Some(owner_username),
        owner_display_name: Some(owner_display_name),
    }))
}

fn is_supported_task_runner_mcp_profile(value: &str) -> bool {
    value.eq_ignore_ascii_case("chatos_plan")
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Result<Option<String>, String> {
    headers
        .get(key)
        .map(|value| {
            value
                .to_str()
                .map(str::trim)
                .map(ToOwned::to_owned)
                .map_err(|_| format!("{key} header format is invalid"))
        })
        .transpose()
        .map(|value| value.filter(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;
    use crate::config::AppConfig;

    fn test_principal(principal_type: &str, id: &str, owner_user_id: Option<&str>) -> CurrentUser {
        CurrentUser {
            principal_type: principal_type.to_string(),
            id: id.to_string(),
            username: format!("{id}-name"),
            display_name: format!("{id} display"),
            role: UserRole::Agent,
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: owner_user_id.map(|value| format!("{value}-name")),
            owner_display_name: owner_user_id.map(|value| format!("{value} display")),
        }
    }

    #[test]
    fn mcp_user_token_header_is_optional_at_parse_layer() {
        let headers = HeaderMap::new();
        assert_eq!(user_access_token_from_headers(&headers).unwrap(), None);
    }

    #[test]
    fn mcp_real_user_token_header_is_read_from_bearer_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-chatos-user-authorization",
            HeaderValue::from_static("Bearer real-user-token"),
        );

        assert_eq!(
            user_access_token_from_headers(&headers).unwrap().as_deref(),
            Some("real-user-token")
        );
    }

    #[test]
    fn mcp_agent_and_user_tokens_must_share_owner_scope() {
        let agent = test_principal("agent_account", "agent-1", Some("user-1"));
        let same_owner = test_principal("human_user", "user-1", Some("user-1"));
        let other_owner = test_principal("human_user", "user-2", Some("user-2"));
        let missing_owner = test_principal("agent_account", "agent-2", None);

        assert!(ensure_same_owner_scope(&agent, &same_owner).is_ok());
        assert_eq!(
            ensure_same_owner_scope(&agent, &other_owner).unwrap_err(),
            "agent token and user token owner scope do not match"
        );
        assert_eq!(
            ensure_same_owner_scope(&missing_owner, &same_owner).unwrap_err(),
            "agent token missing owner scope"
        );
    }

    #[test]
    fn task_runner_internal_mcp_user_accepts_valid_plan_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("sync-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );
        headers.insert(
            "x-task-runner-owner-username",
            HeaderValue::from_static("owner-name"),
        );
        headers.insert(
            "x-task-runner-owner-display-name",
            HeaderValue::from_static("Owner Name"),
        );

        let user = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect("internal user")
            .expect("present");

        assert_eq!(user.principal_type, "human_user");
        assert_eq!(user.id, "user-1");
        assert_eq!(user.username, "owner-name");
        assert_eq!(user.display_name, "Owner Name");
        assert_eq!(user.effective_owner_user_id(), Some("user-1"));
    }

    #[test]
    fn task_runner_internal_mcp_user_rejects_non_plan_profile() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("sync-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("default"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );

        let err = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect_err("non-plan profile should fail");

        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(
            err.message,
            "task runner MCP sync branch only supports chatos_plan"
        );
    }

    #[test]
    fn task_runner_internal_mcp_user_rejects_invalid_sync_secret() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("wrong-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );

        let err = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect_err("invalid secret should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "invalid project sync secret");
    }

    #[test]
    fn task_runner_internal_mcp_user_accepts_signed_scoped_token() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config.internal_api_secrets.insert(
            TASK_RUNNER_CALLER.to_string(),
            "a-long-task-runner-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-secret",
            TASK_RUNNER_CALLER,
            crate::api::internal_auth::PROJECT_SERVICE_TOKEN_AUDIENCE,
            PROJECT_MCP_SCOPE,
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
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );

        let user = task_runner_internal_mcp_user(&config, &headers)
            .expect("signed internal user")
            .expect("present");
        assert_eq!(user.id, "user-1");
    }

    #[test]
    fn task_runner_internal_mcp_user_requires_owner_user_id() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-sync-secret",
            HeaderValue::from_static("sync-secret"),
        );
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );

        let err = task_runner_internal_mcp_user(&test_config(), &headers)
            .expect_err("missing owner user id should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "task runner MCP missing owner user id");
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url:
                "mongodb://admin:admin@127.0.0.1:27018/project_management_test?authSource=admin"
                    .to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_service_request_timeout: Duration::from_millis(5_000),
            memory_engine_base_url: "http://127.0.0.1:7081/api/memory-engine/v1".to_string(),
            memory_engine_source_id: "project_management_agent".to_string(),
            memory_engine_operator_token: None,
            memory_engine_request_timeout: Duration::from_millis(5_000),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            sandbox_image_mcp_request_timeout: Duration::from_millis(5_000),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024 * 1024,
            cloud_project_max_unpacked_bytes: 1024 * 1024,
            cloud_project_max_files: 100,
            cloud_project_git_timeout: Duration::from_millis(5_000),
            task_runner_base_url: Some("http://127.0.0.1:39090".to_string()),
            task_runner_request_timeout: Duration::from_millis(10_000),
            task_runner_internal_secret: Some("sync-secret".to_string()),
            sync_secret: Some("sync-secret".to_string()),
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
        }
    }
}
