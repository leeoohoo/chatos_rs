// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::Value;

use crate::api::internal_auth::{
    record_project_internal_resource_access, require_project_internal_request,
    ProjectInternalRequestIdentity, ProjectInternalResourceAudit, CHATOS_CALLER,
    MCP_MANAGEMENT_CALLER, PROJECT_MCP_SCOPE, TASK_RUNNER_CALLER,
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
    let (current_user, internal_identity) =
        match task_runner_internal_mcp_auth(&state.config, &headers) {
            Ok(Some((user, identity))) => (user, Some(identity)),
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
                let current_user = match verify_token_via_user_service(&state.config, &token).await
                {
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
                let user =
                    match verify_token_via_user_service(&state.config, user_access_token).await {
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
                (current_user.with_owner_identity_from(&user), None)
            }
            Err(err) => {
                return Json(mcp_server::jsonrpc_error_response(
                    err.status,
                    id,
                    err.message,
                ));
            }
        };
    let project_id = match project_id_from_headers(&headers) {
        Ok(value) => value,
        Err(message) => {
            return Json(mcp_server::jsonrpc_error_response(
                StatusCode::FORBIDDEN,
                id,
                message,
            ));
        }
    };
    let mutation = internal_identity.as_ref().and_then(|identity| {
        project_id.as_deref().and_then(|project_id| {
            requested_project_mcp_mutation(&request, project_id).map(|mutation| {
                (
                    identity.clone(),
                    current_user
                        .effective_owner_user_id()
                        .map(ToOwned::to_owned),
                    project_id.to_string(),
                    mutation,
                )
            })
        })
    });
    let response = mcp_server::handle_jsonrpc(state, current_user, project_id, request).await;
    if let Some((identity, represented_user_id, project_id, mutation)) = mutation {
        record_project_internal_resource_access(
            &identity,
            ProjectInternalResourceAudit {
                represented_user_id: represented_user_id.as_deref(),
                project_id: Some(project_id.as_str()),
                resource_type: mutation.resource_type,
                resource_id: mutation.resource_id.as_str(),
                resource_name: Some(mutation.tool_name),
                action: mutation.tool_name,
                outcome: if response.error.is_some() {
                    "failed"
                } else {
                    "succeeded"
                },
            },
        );
    }
    Json(response)
}

struct ProjectMcpMutation {
    tool_name: &'static str,
    resource_type: &'static str,
    resource_id: String,
}

fn requested_project_mcp_mutation(
    request: &JsonRpcRequest,
    project_id: &str,
) -> Option<ProjectMcpMutation> {
    use chatos_mcp::project_management_contract::tools;

    if request.method != "tools/call" {
        return None;
    }
    let params = request.params.as_ref()?;
    let tool_name = params.get("name")?.as_str()?.trim();
    let arguments = params.get("arguments").and_then(Value::as_object);
    let argument = |key: &str| {
        arguments
            .and_then(|arguments| arguments.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let mutation = match tool_name {
        tools::INITIALIZE_PROJECT => ProjectMcpMutation {
            tool_name: tools::INITIALIZE_PROJECT,
            resource_type: "project",
            resource_id: project_id.to_string(),
        },
        tools::CREATE_REQUIREMENT => ProjectMcpMutation {
            tool_name: tools::CREATE_REQUIREMENT,
            resource_type: "requirement_collection",
            resource_id: project_id.to_string(),
        },
        tools::UPDATE_REQUIREMENT
        | tools::DELETE_REQUIREMENT
        | tools::SET_REQUIREMENT_DEPENDENCIES => ProjectMcpMutation {
            tool_name: match tool_name {
                tools::UPDATE_REQUIREMENT => tools::UPDATE_REQUIREMENT,
                tools::DELETE_REQUIREMENT => tools::DELETE_REQUIREMENT,
                _ => tools::SET_REQUIREMENT_DEPENDENCIES,
            },
            resource_type: "requirement",
            resource_id: argument("requirement_id")?.to_string(),
        },
        tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT => ProjectMcpMutation {
            tool_name: tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT,
            resource_type: "requirement_technical_document",
            resource_id: argument("document_id")
                .or_else(|| argument("requirement_id"))?
                .to_string(),
        },
        tools::CREATE_PROJECT_TASK => ProjectMcpMutation {
            tool_name: tools::CREATE_PROJECT_TASK,
            resource_type: "project_task_collection",
            resource_id: project_id.to_string(),
        },
        tools::UPDATE_PROJECT_TASK
        | tools::DELETE_PROJECT_TASK
        | tools::SET_PROJECT_TASK_DEPENDENCIES => ProjectMcpMutation {
            tool_name: match tool_name {
                tools::UPDATE_PROJECT_TASK => tools::UPDATE_PROJECT_TASK,
                tools::DELETE_PROJECT_TASK => tools::DELETE_PROJECT_TASK,
                _ => tools::SET_PROJECT_TASK_DEPENDENCIES,
            },
            resource_type: "project_task",
            resource_id: argument("project_task_id")?.to_string(),
        },
        _ => return None,
    };
    Some(mutation)
}

fn project_id_from_headers(headers: &HeaderMap) -> Result<Option<String>, String> {
    let chatos_project_id = header_text(headers, "x-chatos-project-id")?;
    let mcp_management_project_id = header_text(headers, "x-mcp-management-project-id")?;
    if let (Some(chatos_project_id), Some(mcp_management_project_id)) =
        (&chatos_project_id, &mcp_management_project_id)
    {
        if chatos_project_id != mcp_management_project_id {
            return Err(
                "MCP Management project id does not match the project request header".to_string(),
            );
        }
    }
    Ok(mcp_management_project_id.or(chatos_project_id))
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

fn task_runner_internal_mcp_auth(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
) -> Result<Option<(CurrentUser, ProjectInternalRequestIdentity)>, ApiError> {
    let caller = header_text(headers, "x-project-service-caller").map_err(ApiError::bad_request)?;
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
    let identity = require_project_internal_request(
        config,
        headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER, MCP_MANAGEMENT_CALLER],
        PROJECT_MCP_SCOPE,
    )?;
    if caller.as_deref() == Some(MCP_MANAGEMENT_CALLER) {
        return mcp_management_internal_user(headers).map(|user| Some((user, identity)));
    }
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
    Ok(Some((
        CurrentUser {
            principal_type: "human_user".to_string(),
            id: owner_user_id.clone(),
            username: owner_username.clone(),
            display_name: owner_display_name.clone(),
            role: UserRole::Agent,
            owner_user_id: Some(owner_user_id),
            owner_username: Some(owner_username),
            owner_display_name: Some(owner_display_name),
        },
        identity,
    )))
}

#[cfg(test)]
fn task_runner_internal_mcp_user(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
) -> Result<Option<CurrentUser>, ApiError> {
    task_runner_internal_mcp_auth(config, headers).map(|auth| auth.map(|(user, _identity)| user))
}

fn mcp_management_internal_user(headers: &HeaderMap) -> Result<CurrentUser, ApiError> {
    let owner_user_id = header_text(headers, "x-mcp-management-owner-user-id")
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::unauthorized("MCP Management owner user id is required"))?;
    let agent_key = header_text(headers, "x-mcp-management-agent-key")
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::unauthorized("MCP Management Agent key is required"))?;
    if !chatos_plugin_management_sdk::SystemAgentKey::ALL
        .into_iter()
        .any(|key| key.as_str() == agent_key)
    {
        return Err(ApiError::forbidden(
            "MCP Management Agent key is not registered",
        ));
    }
    header_text(headers, "x-mcp-management-session-id")
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::unauthorized("MCP Management session id is required"))?;
    Ok(CurrentUser {
        principal_type: "human_user".to_string(),
        id: owner_user_id.clone(),
        username: owner_user_id.clone(),
        display_name: owner_user_id.clone(),
        role: UserRole::Agent,
        owner_user_id: Some(owner_user_id.clone()),
        owner_username: Some(owner_user_id.clone()),
        owner_display_name: Some(owner_user_id),
    })
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
    fn internal_mcp_audit_selects_only_mutations_and_resource_ids() {
        let request = |name: &str, arguments: Value| JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": name,
                "arguments": arguments,
            })),
        };

        let update = requested_project_mcp_mutation(
            &request(
                "update_requirement",
                serde_json::json!({
                    "requirement_id": "requirement-1",
                    "patch": { "detail": "private body" },
                }),
            ),
            "project-1",
        )
        .expect("mutating tool");
        assert_eq!(update.resource_type, "requirement");
        assert_eq!(update.resource_id, "requirement-1");
        assert_eq!(update.tool_name, "update_requirement");

        assert!(requested_project_mcp_mutation(
            &request("list_requirements", serde_json::json!({})),
            "project-1",
        )
        .is_none());
    }

    #[test]
    fn task_runner_internal_mcp_user_accepts_valid_plan_headers() {
        let (config, mut headers) = signed_task_runner_request("task-runner-secret");
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

        let user = task_runner_internal_mcp_user(&config, &headers)
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
        let (config, mut headers) = signed_task_runner_request("task-runner-secret");
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("default"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );

        let err = task_runner_internal_mcp_user(&config, &headers)
            .expect_err("non-plan profile should fail");

        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(
            err.message,
            "task runner MCP sync branch only supports chatos_plan"
        );
    }

    #[test]
    fn task_runner_internal_mcp_user_rejects_invalid_signed_token() {
        let (config, mut headers) = signed_task_runner_request("wrong-secret");
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );
        headers.insert(
            "x-task-runner-owner-user-id",
            HeaderValue::from_static("user-1"),
        );

        let mut config = config;
        config
            .internal_api_secrets
            .insert(TASK_RUNNER_CALLER.to_string(), "correct-secret".to_string());
        let err = task_runner_internal_mcp_user(&config, &headers)
            .expect_err("invalid signed token should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "invalid project service internal API token");
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
        let (config, mut headers) = signed_task_runner_request("task-runner-secret");
        headers.insert(
            "x-task-runner-task-profile",
            HeaderValue::from_static("chatos_plan"),
        );

        let err = task_runner_internal_mcp_user(&config, &headers)
            .expect_err("missing owner user id should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "task runner MCP missing owner user id");
    }

    fn signed_task_runner_request(secret: &str) -> (crate::config::AppConfig, HeaderMap) {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config
            .internal_api_secrets
            .insert(TASK_RUNNER_CALLER.to_string(), secret.to_string());
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
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
        (config, headers)
    }

    #[test]
    fn mcp_management_internal_mcp_user_accepts_signed_session_identity() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config.internal_api_secrets.insert(
            MCP_MANAGEMENT_CALLER.to_string(),
            "a-long-mcp-management-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-mcp-management-secret",
            MCP_MANAGEMENT_CALLER,
            crate::api::internal_auth::PROJECT_SERVICE_TOKEN_AUDIENCE,
            PROJECT_MCP_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-project-service-caller",
            HeaderValue::from_static(MCP_MANAGEMENT_CALLER),
        );
        headers.insert(
            "x-project-service-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );
        headers.insert(
            "x-mcp-management-owner-user-id",
            HeaderValue::from_static("user-1"),
        );
        headers.insert(
            "x-mcp-management-agent-key",
            HeaderValue::from_static("task_runner_run_phase"),
        );
        headers.insert(
            "x-mcp-management-session-id",
            HeaderValue::from_static("session-1"),
        );

        let user = task_runner_internal_mcp_user(&config, &headers)
            .expect("signed internal user")
            .expect("present");
        assert_eq!(user.id, "user-1");
        assert_eq!(user.effective_owner_user_id(), Some("user-1"));
    }

    #[test]
    fn mcp_management_project_headers_must_match() {
        let mut headers = HeaderMap::new();
        headers.insert("x-chatos-project-id", HeaderValue::from_static("project-1"));
        headers.insert(
            "x-mcp-management-project-id",
            HeaderValue::from_static("project-2"),
        );
        assert!(project_id_from_headers(&headers).is_err());
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url:
                "mongodb://admin:admin@127.0.0.1:27018/project_management_test?authSource=admin"
                    .to_string(),
            mcp_result_rabbitmq_url: "amqp://127.0.0.1:1/%2f".to_string(),
            mcp_result_queue_prefix: "project_service.mcp.results.test".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_internal_base_url: "https://127.0.0.1:39192".to_string(),
            user_service_internal_http_client: reqwest::Client::new(),
            user_service_request_timeout: Duration::from_millis(5_000),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_http_client: reqwest::Client::new(),
            local_connector_service_request_timeout: Duration::from_millis(5_000),
            memory_engine_base_url: "http://127.0.0.1:7081/api/memory-engine/v1".to_string(),
            memory_engine_http_client: reqwest::Client::new(),
            memory_engine_source_id: "project_management_agent".to_string(),
            memory_engine_internal_api_secret: None,
            memory_engine_request_timeout: Duration::from_millis(5_000),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_http_client: reqwest::Client::new(),
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
