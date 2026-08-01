// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::{Extension, Json};
use chatos_mcp_management_sdk::{
    CloseRuntimeSessionResponse, CreateRuntimeSessionRequest, RuntimeSessionResponse,
    RuntimeSessionRoutesResponse,
};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde::Deserialize;

use chatos_service_runtime::http_body::{
    read_response_bytes_limited, DEFAULT_RESPONSE_BODY_LIMIT_BYTES,
};

use crate::models::{CurrentUser, BINDING_MODE_MCP};
use crate::state::AppState;

use super::{required_text, validate_device_workspace, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct ResolveCommandApprovalRuntimeSessionRequest {
    project_id: Option<String>,
    device_id: Option<String>,
    workspace_id: Option<String>,
    run_id: Option<String>,
    model_config_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CloseCommandApprovalRuntimeSessionRequest {
    project_id: Option<String>,
    run_id: Option<String>,
}

pub(super) async fn resolve_command_approval_runtime_session(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(request): Json<ResolveCommandApprovalRuntimeSessionRequest>,
) -> Result<Json<RuntimeSessionResponse>, ApiError> {
    let project_id = required_text(request.project_id, "project_id")?;
    let device_id = required_text(request.device_id, "device_id")?;
    let workspace_id = required_text(request.workspace_id, "workspace_id")?;
    let run_id = required_text(request.run_id, "run_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    require_command_approval_project_binding(
        &state,
        &user,
        project_id.as_str(),
        device_id.as_str(),
        workspace_id.as_str(),
    )
    .await?;

    let session_request = command_approval_session_request(
        user.effective_owner_user_id(),
        project_id.as_str(),
        device_id.as_str(),
        run_id.as_str(),
        request.model_config_id.as_deref(),
    );
    let mut session = state
        .mcp_management_client
        .resolve_runtime_session(&session_request)
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!(
                "resolve command approval MCP Management session failed: {error}"
            ))
        })?;
    session.mcp_server_url = state.config.mcp_management_runtime_facade_url();
    Ok(Json(session))
}

pub(super) async fn mcp_management_runtime_proxy(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if body.len() > DEFAULT_RESPONSE_BODY_LIMIT_BYTES {
        return Err(ApiError::bad_request(
            "MCP Management runtime request body is too large",
        ));
    }
    let authorization = required_runtime_grant(&headers)?;
    let response = forward_mcp_management_runtime_request(
        state.mcp_management_runtime_http(),
        state.mcp_management_client.config().base_url.as_str(),
        authorization,
        body,
    )
    .await?;
    let status = StatusCode::from_u16(response.status().as_u16()).map_err(|error| {
        ApiError::bad_gateway(format!("MCP Management returned invalid status: {error}"))
    })?;
    let bytes = read_response_bytes_limited(response, DEFAULT_RESPONSE_BODY_LIMIT_BYTES)
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!(
                "read MCP Management runtime response failed: {error}"
            ))
        })?;
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .map_err(|error| {
            ApiError::internal(format!(
                "build MCP Management runtime response failed: {error}"
            ))
        })
}

fn required_runtime_grant(headers: &HeaderMap) -> Result<HeaderValue, ApiError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .cloned()
        .ok_or_else(|| ApiError::unauthorized("MCP runtime grant is required"))?;
    let value = authorization
        .to_str()
        .map_err(|_| ApiError::unauthorized("MCP runtime grant is invalid"))?;
    let token = value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unauthorized("MCP runtime grant must use Bearer authentication")
        })?;
    if token.contains(char::is_whitespace) {
        return Err(ApiError::unauthorized("MCP runtime grant is invalid"));
    }
    Ok(authorization)
}

async fn forward_mcp_management_runtime_request(
    http: &reqwest::Client,
    base_url: &str,
    authorization: HeaderValue,
    body: Bytes,
) -> Result<reqwest::Response, ApiError> {
    http.post(format!("{}/mcp", base_url.trim_end_matches('/')))
        .header(AUTHORIZATION, authorization)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("MCP Management runtime request failed: {error}"))
        })
}

pub(super) async fn close_command_approval_runtime_session(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(session_id): Path<String>,
    Json(request): Json<CloseCommandApprovalRuntimeSessionRequest>,
) -> Result<Json<CloseRuntimeSessionResponse>, ApiError> {
    let project_id = required_text(request.project_id, "project_id")?;
    let run_id = required_text(request.run_id, "run_id")?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(ApiError::bad_request("session_id is required"));
    }
    let session = state
        .mcp_management_client
        .runtime_session_routes(session_id)
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!(
                "read command approval MCP Management session failed: {error}"
            ))
        })?;
    validate_command_approval_session_identity(
        &session,
        user.effective_owner_user_id(),
        project_id.as_str(),
        run_id.as_str(),
    )?;
    state
        .mcp_management_client
        .close_runtime_session(session_id)
        .await
        .map(Json)
        .map_err(|error| {
            ApiError::bad_gateway(format!(
                "close command approval MCP Management session failed: {error}"
            ))
        })
}

async fn require_command_approval_project_binding(
    state: &AppState,
    user: &CurrentUser,
    project_id: &str,
    device_id: &str,
    workspace_id: &str,
) -> Result<(), ApiError> {
    let bindings = state
        .store
        .list_project_bindings(
            user.effective_owner_user_id(),
            Some(project_id.to_string()),
            Some(BINDING_MODE_MCP.to_string()),
        )
        .await
        .map_err(ApiError::internal)?;
    if bindings.iter().any(|binding| {
        binding.enabled && binding.device_id == device_id && binding.workspace_id == workspace_id
    }) {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "project is not bound to this Local Connector MCP workspace",
    ))
}

fn command_approval_session_request(
    owner_user_id: &str,
    project_id: &str,
    device_id: &str,
    run_id: &str,
    model_config_id: Option<&str>,
) -> CreateRuntimeSessionRequest {
    CreateRuntimeSessionRequest {
        owner_user_id: owner_user_id.trim().to_string(),
        agent_key: SystemAgentKey::LocalConnectorCommandApprovalAgent
            .as_str()
            .to_string(),
        project_id: project_id.trim().to_string(),
        run_id: Some(run_id.trim().to_string()),
        turn_id: None,
        task_id: None,
        task_profile: None,
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: model_config_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        expected_project_task_ids: Vec::new(),
        locale: None,
        requested_device_id: Some(device_id.trim().to_string()),
        requested_sandbox_provider: None,
        sandbox_target: None,
    }
}

fn validate_command_approval_session_identity(
    session: &RuntimeSessionRoutesResponse,
    owner_user_id: &str,
    project_id: &str,
    run_id: &str,
) -> Result<(), ApiError> {
    if session.owner_user_id != owner_user_id.trim()
        || session.agent_key != SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str()
        || session.project_id != project_id.trim()
        || session.run_id.as_deref() != Some(run_id.trim())
    {
        return Err(ApiError::forbidden(
            "runtime session does not belong to this command approval run",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::Router;
    use serde_json::json;

    #[test]
    fn runtime_request_fixes_agent_owner_project_run_model_and_device() {
        let request = command_approval_session_request(
            "user-1",
            "project-1",
            "device-1",
            "approval-run-1",
            Some("model-1"),
        );

        assert_eq!(request.owner_user_id, "user-1");
        assert_eq!(request.project_id, "project-1");
        assert_eq!(
            request.agent_key,
            SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str()
        );
        assert_eq!(request.run_id.as_deref(), Some("approval-run-1"));
        assert_eq!(request.default_model_config_id.as_deref(), Some("model-1"));
        assert_eq!(request.requested_device_id.as_deref(), Some("device-1"));
        assert!(request.requested_sandbox_provider.is_none());
        assert!(request.sandbox_target.is_none());
    }

    #[test]
    fn runtime_facade_requires_a_nonempty_bearer_grant() {
        let missing = required_runtime_grant(&HeaderMap::new()).expect_err("missing grant");
        assert_eq!(missing.message(), "MCP runtime grant is required");

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic credentials"));
        let wrong_scheme = required_runtime_grant(&headers).expect_err("wrong scheme");
        assert_eq!(
            wrong_scheme.message(),
            "MCP runtime grant must use Bearer authentication"
        );

        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer runtime-token"),
        );
        assert_eq!(
            required_runtime_grant(&headers)
                .expect("valid grant")
                .to_str()
                .expect("header text"),
            "Bearer runtime-token"
        );
    }

    #[tokio::test]
    async fn runtime_facade_forwards_only_grant_and_json_body_to_fixed_mcp_path() {
        async fn handler(headers: HeaderMap, body: Bytes) -> Json<serde_json::Value> {
            assert_eq!(
                headers
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer runtime-token")
            );
            assert_eq!(
                headers
                    .get("content-type")
                    .and_then(|value| value.to_str().ok()),
                Some("application/json")
            );
            assert!(!headers.contains_key("x-untrusted-forwarded-header"));
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(body.as_ref()).expect("JSON body"),
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
            );
            Json(json!({"jsonrpc": "2.0", "id": 1, "result": {"tools": []}}))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake MCP Management");
        let address = listener.local_addr().expect("fake MCP Management address");
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", post(handler)))
                .await
                .expect("serve fake MCP Management");
        });
        let mut inbound_headers = HeaderMap::new();
        inbound_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer runtime-token"),
        );
        inbound_headers.insert(
            "x-untrusted-forwarded-header",
            HeaderValue::from_static("must-not-leak"),
        );
        let body = Bytes::from_static(br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        let response = forward_mcp_management_runtime_request(
            &reqwest::Client::new(),
            format!("http://{address}").as_str(),
            required_runtime_grant(&inbound_headers).expect("runtime grant"),
            body,
        )
        .await
        .expect("forward MCP request");
        assert!(response.status().is_success());
        server.abort();
    }
}
